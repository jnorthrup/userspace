use anyhow::Result;
use userspace::database::lsmr::{LsmrConfig, LsmrDatabase};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn recovery_from_tmp_files() -> Result<()> {
    // create temp dir
    let td = tempdir()?;
    let path = td.path().to_path_buf();

    let cfg = LsmrConfig { path: path.clone(), memtable_threshold: 1, max_segments: None };
    let db = LsmrDatabase::open(cfg.clone()).await?;

    // simulate leftover tmp files from an interrupted flush/compaction
    let tmp_data = path.join("segment_00000000000000000001.tmp.data");
    let tmp_meta = path.join("segment_00000000000000000001.tmp.meta.json");
    std::fs::write(&tmp_data, b"corrupt")?;
    std::fs::write(&tmp_meta, b"{\"filename\":\"segment_00000000000000000001.data\",\"index\":{},\"size\":0}")?;

    // reopen DB which should cleanup tmp files
    drop(db);
    let db2 = LsmrDatabase::open(cfg).await?;

    // make sure tmp files are removed
    assert!(!tmp_data.exists());
    assert!(!tmp_meta.exists());

    // basic put/get still works
    db2.put_json("k1".to_string(), &json!({"v":1})).await?;
    let got = db2.get("k1").await?;
    assert_eq!(got.unwrap(), json!({"v":1}));

    Ok(())
}
