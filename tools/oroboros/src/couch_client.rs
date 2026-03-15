use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{sleep, Duration};
use rand::Rng;
use urlencoding::encode;

#[derive(Clone)]
pub struct CouchConfig {
    pub base: String,
    pub db: String,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub concurrency: usize,
    pub max_retries: usize,
}

pub struct CouchClient {
    client: Client,
    cfg: Arc<CouchConfig>,
    sem: Arc<Semaphore>,
}

impl CouchClient {
    pub fn new(client: Client, cfg: CouchConfig) -> Self {
    let sem = Arc::new(Semaphore::new(cfg.concurrency.max(1)));
    Self { client, cfg: Arc::new(cfg), sem }
    }

    async fn auth_req(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let (Some(u), Some(p)) = (self.cfg.user.as_ref(), self.cfg.pass.as_ref()) {
            req = req.basic_auth(u, Some(p));
        }
        req
    }

    pub async fn get_doc_rev(&self, docid: &str) -> Result<Option<String>> {
    // encode docid to allow slashes and special chars
    let docid_enc = encode(docid);
    let url = format!("{}/{}/{}", self.cfg.base.trim_end_matches('/'), self.cfg.db, docid_enc);
    let mut attempt = 0usize;
    loop {
        attempt += 1;
        let mut req = self.client.get(&url);
            req = self.auth_req(req).await;
            let resp = req.send().await;
            match resp {
                Ok(r) => {
                    let status = r.status();
                    if status.is_success() {
                        let v: serde_json::Value = r.json().await?;
                        return Ok(v.get("_rev").and_then(|r| r.as_str()).map(|s| s.to_string()));
                    } else if status.as_u16() == 404 {
                        return Ok(None);
                    } else if attempt >= self.cfg.max_retries {
                        let body = r.text().await.unwrap_or_default();
                        return Err(anyhow!("get_doc_rev failed status={} body={}", status, body));
                    }
                }
                Err(e) => {
            if attempt >= self.cfg.max_retries { return Err(anyhow!(e)); }
                }
            }
        // Exponential backoff with jitter
        let base = 150u64;
        let backoff = base.saturating_mul(2u64.pow(attempt.saturating_sub(1) as u32));
        let jitter: u64 = rand::thread_rng().gen_range(0..(base));
        let wait = std::cmp::min(backoff + jitter, 10_000);
        sleep(Duration::from_millis(wait)).await;
        }
    }

    pub async fn create_doc_if_missing(&self, docid: &str) -> Result<String> {
        // attempt to create a doc with the given id; returns new _rev
        let docid_enc = encode(docid);
        let url = format!("{}/{}/{}", self.cfg.base.trim_end_matches('/'), self.cfg.db, docid_enc);
        let mut attempt = 0usize;
        loop {
            attempt += 1;
            let mut req = self.client.put(&url).json(&json!({}));
            req = self.auth_req(req).await;
            match req.send().await {
                Ok(r) => {
                    let status = r.status();
                    if status.is_success() {
                        let v: serde_json::Value = r.json().await?;
                        if let Some(rev) = v.get("rev").and_then(|r| r.as_str()) { return Ok(rev.to_string()); }
                    } else if status.as_u16() == 409 {
                        // already created by someone else; fetch rev
                        if let Some(rev) = self.get_doc_rev(docid).await? { return Ok(rev); }
                    } else if attempt >= self.cfg.max_retries {
                        let body = r.text().await.unwrap_or_default();
                        return Err(anyhow!("create_doc failed {}", body));
                    }
                }
                Err(e) => if attempt >= self.cfg.max_retries { return Err(anyhow!(e)); }
            }
            let base = 200u64;
            let backoff = base.saturating_mul(2u64.pow(attempt.saturating_sub(1) as u32));
            let jitter: u64 = rand::thread_rng().gen_range(0..(base));
            let wait = std::cmp::min(backoff + jitter, 10_000);
            sleep(Duration::from_millis(wait)).await;
        }
    }

    pub async fn put_attachment(&self, docid: &str, name: &str, data: Vec<u8>, content_type: &str, rev_opt: Option<String>) -> Result<String> {
        let _permit = self.sem.acquire().await.unwrap();
        let mut attempt = 0usize;
        loop {
            attempt += 1;
            let name_enc = encode(name);
            let docid_enc = encode(docid);
            let mut url = format!("{}/{}/{}/{}", self.cfg.base.trim_end_matches('/'), self.cfg.db, docid_enc, name_enc);
            if let Some(r) = rev_opt.as_ref() { url = format!("{}?rev={}", url, r); }

            let mut req = self.client.put(&url).body(data.clone()).header("Content-Type", content_type);
            req = self.auth_req(req).await;

            match req.send().await {
                Ok(r) => {
                    let status = r.status();
                    if status.is_success() {
                        let v: serde_json::Value = r.json().await?;
                        if let Some(rev) = v.get("rev").and_then(|r| r.as_str()) { return Ok(rev.to_string()); }
                        return Err(anyhow!("put_attachment missing rev in response"));
                    } else if status.as_u16() == 409 {
                        // conflict: caller can fetch latest rev and retry
                    } else if attempt >= self.cfg.max_retries {
                        let body = r.text().await.unwrap_or_default();
                        return Err(anyhow!("put_attachment failed {}", body));
                    }
                }
                Err(e) => {
                    if attempt >= self.cfg.max_retries { return Err(anyhow!(e)); }
                }
            }

            let base = 120u64;
            let backoff = base.saturating_mul(2u64.pow(attempt.saturating_sub(1) as u32));
            let jitter: u64 = rand::thread_rng().gen_range(0..(base));
            let wait = std::cmp::min(backoff + jitter, 8_000);
            sleep(Duration::from_millis(wait)).await;
        }
    }
}
