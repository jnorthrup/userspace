use anyhow::Result;
use userspace::database::lsmr::{LsmrConfig, LsmrDatabase};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn tombstone_compaction() -> Result<()> {
    let td = tempdir()?;
    let path = td.path().to_path_buf();
    let cfg = LsmrConfig { path: path.clone(), memtable_threshold: 1, max_segments: None };
    let db = LsmrDatabase::open(cfg.clone()).await?;

    // insert key and flush
    db.put_json("k1".to_string(), &json!({"v":1})).await?;
    // delete key
    db.delete("k1".to_string()).await?;

    // force compaction
    db.compact_merge().await?;

    // after compaction, key should be gone
    let got = db.get("k1").await?;
    assert!(got.is_none());
    Ok(())
}
