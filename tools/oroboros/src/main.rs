use anyhow::Result;
use futures::stream::{FuturesUnordered, StreamExt};
use mime_guess::from_path;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use reqwest::Client;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;
use oroboros::couch_client::{CouchClient, CouchConfig};
use oroboros::api_client::GeneratedClient;
use serde::Deserialize;

/// Minimal oroboros: batch file changes and replace attachments in CouchDB 1.7.2
/// Assumptions:
/// - Files in the watched directory follow the path convention: <docid>/<attachment-name>
/// - CouchDB URL, db name, and optional basic auth provided via env vars or CLI args

#[derive(Clone)]
struct ConfigArgs {
    couch_url: String,
    db: String,
    username: Option<String>,
    password: Option<String>,
    watch_dir: PathBuf,
    batch_interval_ms: u64,
}

#[derive(Deserialize)]
struct CouchDoc { _id: String, _rev: String }

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (cfg, client_cfg) = parse_args(&args)?;

    println!("oroboros: watching {} -> {}/{} (concurrency={} retries={})", cfg.watch_dir.display(), client_cfg.base, client_cfg.db, client_cfg.concurrency, client_cfg.max_retries);

    let (tx, mut rx) = mpsc::channel::<PathBuf>(1024);

    // Spawn file watcher
    let watch_dir = cfg.watch_dir.clone();
    let tx2 = tx.clone();
    std::thread::spawn(move || {
        if let Err(e) = blocking_watch(watch_dir, tx2) {
            eprintln!("watcher error: {}", e);
        }
    });

    // Collect modified files into batches keyed by doc id
    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
    let cfg = Arc::new(cfg);
    let couch_client = Arc::new(CouchClient::new(client.clone(), client_cfg));
    // expose a tiny generated client for other tools/tests
    let _api = GeneratedClient::new(couch_client.clone());

    let mut pending: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut last_flush = Instant::now();

    loop {
        tokio::select! {
            Some(path) = rx.recv() => {
                if let Some((docid, _attach)) = path_to_doc_and_name(&path) {
                    pending.entry(docid).or_default().push(path);
                }
            }
            _ = sleep(Duration::from_millis(cfg.batch_interval_ms)) => {
                // time to flush if any
            }
        }

        // flush if interval elapsed
        if last_flush.elapsed() >= Duration::from_millis(cfg.batch_interval_ms) && !pending.is_empty() {
            let batch = std::mem::take(&mut pending);
            last_flush = Instant::now();
            let client = client.clone();
            let cfg = cfg.clone();
            let couch_client = couch_client.clone();
            tokio::spawn(async move {
                if let Err(e) = process_batch(client, cfg, couch_client, batch).await {
                    eprintln!("batch upload failed: {}", e);
                }
            });
        }
    }
}

fn parse_args(argv: &[String]) -> Result<(ConfigArgs, CouchConfig)> {
    // Minimal parsing: args --url URL --db DB --watch DIR [--user USER --pass PASS] [--interval ms]
    let mut couch_url = "http://127.0.0.1:5984".to_string();
    let mut db = "mydb".to_string();
    let mut watch_dir = PathBuf::from(".");
    let mut username = None;
    let mut password = None;
    let mut interval = 2000u64;

    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--url" => { i+=1; couch_url = argv[i].clone(); }
            "--db" => { i+=1; db = argv[i].clone(); }
            "--watch" => { i+=1; watch_dir = PathBuf::from(&argv[i]); }
            "--user" => { i+=1; username = Some(argv[i].clone()); }
            "--pass" => { i+=1; password = Some(argv[i].clone()); }
            "--interval" => { i+=1; interval = argv[i].parse().unwrap_or(2000); }
            _ => {}
        }
        i+=1;
    }

    // defaults for couch client
    let mut concurrency = 4usize;
    let mut max_retries = 5usize;
    // quick pass to override concurrency/retries
    let mut j = 1;
    while j < argv.len() {
        match argv[j].as_str() {
            "--concurrency" => { j+=1; concurrency = argv[j].parse().unwrap_or(concurrency); }
            "--retries" => { j+=1; max_retries = argv[j].parse().unwrap_or(max_retries); }
            _ => {}
        }
        j+=1;
    }

    let client_cfg = CouchConfig {
        base: couch_url.clone(),
        db: db.clone(),
        user: username.clone(),
        pass: password.clone(),
        concurrency,
        max_retries,
    };

    Ok((ConfigArgs { couch_url, db, username, password, watch_dir, batch_interval_ms: interval }, client_cfg))
}

fn blocking_watch(watch_dir: PathBuf, tx: mpsc::Sender<PathBuf>) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let (event_tx, event_rx) = std::sync::mpsc::channel();

    let mut watcher: RecommendedWatcher = Watcher::new(event_tx, Config::default())?;
    watcher.watch(&watch_dir, RecursiveMode::Recursive)?;

    for res in event_rx {
        match res {
            Ok(Event{ paths, .. }) => {
                for p in paths {
                    // ignore directories
                    if p.is_file() {
                        let tx = tx.clone();
                        let p2 = p.clone();
                        runtime.block_on(async move {
                            let _ = tx.send(p2).await;
                        });
                    }
                }
            }
            Err(e) => eprintln!("watch error: {:?}", e),
        }
    }

    Ok(())
}

fn path_to_doc_and_name(path: &Path) -> Option<(String, String)> {
    // Expect path like .../<docid>/<attachment>
    let comps: Vec<_> = path.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    if comps.len() < 2 { return None }
    let docid = comps[comps.len()-2].clone();
    let name = comps[comps.len()-1].clone();
    Some((docid, name))
}

async fn process_batch(client: Client, cfg: Arc<ConfigArgs>, couch_client: Arc<CouchClient>, batch: HashMap<String, Vec<PathBuf>>) -> Result<()> {
    let mut futures = FuturesUnordered::new();
    for (docid, paths) in batch.into_iter() {
        let client = client.clone();
        let cfg = cfg.clone();
        let couch_client = couch_client.clone();
        futures.push(tokio::spawn(async move {
            if let Err(e) = upload_attachments_for_doc(&client, &cfg, couch_client, &docid, paths).await {
                eprintln!("doc {} upload error: {}", docid, e);
            }
        }));
    }

    while let Some(_) = futures.next().await {}
    Ok(())
}

async fn get_doc_rev(client: &Client, cfg: &ConfigArgs, docid: &str) -> Result<Option<String>> {
    let url = format!("{}/{}/{}", cfg.couch_url.trim_end_matches('/'), cfg.db, docid);
    let mut req = client.get(&url);
    if let (Some(u), Some(p)) = (cfg.username.as_ref(), cfg.password.as_ref()) {
        req = req.basic_auth(u, Some(p));
    }
    let resp = req.send().await?;
    if resp.status().is_success() {
        let doc: serde_json::Value = resp.json().await?;
        if let Some(rev) = doc.get("_rev").and_then(|v| v.as_str()) {
            return Ok(Some(rev.to_string()));
        }
    }
    Ok(None)
}

async fn upload_attachments_for_doc(client: &Client, cfg: &ConfigArgs, couch_client: Arc<CouchClient>, docid: &str, paths: Vec<PathBuf>) -> Result<()> {
    // Ensure doc exists (create if missing)
    let rev_opt = couch_client.get_doc_rev(docid).await?;
    let mut rev = rev_opt;
    if rev.is_none() {
        let created_rev = couch_client.create_doc_if_missing(docid).await?;
        rev = Some(created_rev);
    }

    for path in paths {
        let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("attachment");
        let mime = from_path(&path).first_or_octet_stream();
        let bytes = tokio::fs::read(&path).await?;

        let current_rev = rev.clone();
        match couch_client.put_attachment(docid, fname, bytes, mime.as_ref(), current_rev).await {
            Ok(new_rev) => { rev = Some(new_rev); println!("uploaded {} to {}/{} rev={:?}", fname, cfg.db, docid, rev); }
            Err(e) => { eprintln!("failed to upload {}: {}", fname, e); }
        }
    }

    Ok(())
}
