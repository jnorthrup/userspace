use anyhow::Result;
use userspace::database::lsmr::{LsmrConfig, LsmrDatabase};
use serde_json::json;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let dir = args.get(1).cloned().unwrap_or_else(|| "./data".to_string());
    let cfg = LsmrConfig { path: std::path::PathBuf::from(dir), memtable_threshold: 1024, max_segments: Some(10) };
    let db = LsmrDatabase::open(cfg).await?;
    db.start_background_compactor().await;

    // simple demo puts
    db.put_json("demo1".to_string(), &json!({"hello":"world"})).await?;
    let v = db.get("demo1").await?;
    println!("got: {:?}", v);
    Ok(())
}
