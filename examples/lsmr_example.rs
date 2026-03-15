use userspace::database::lsmr::{LsmrConfig, LsmrDatabase};
use serde_json::json;
use std::env;

#[tokio::main]
async fn main() {
    let dir = env::args().nth(1).unwrap_or_else(|| ".".into());
    let cfg = LsmrConfig { path: std::path::PathBuf::from(dir), memtable_threshold: 1024, max_segments: Some(10) };
    let db = LsmrDatabase::open(cfg).await.unwrap();
    db.start_background_compactor().await;

    // demo write
    db.put_json("demo1".to_string(), &json!({"hello":"world"})).await.unwrap();
    let got = db.get("demo1").await.unwrap();
    println!("got: {:?}", got);
}
