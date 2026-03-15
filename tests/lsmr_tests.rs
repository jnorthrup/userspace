use userspace::database::lsmr::{LsmrConfig, LsmrDatabase};
use tempfile::tempdir;
use serde_json::json;

#[tokio::test]
async fn happy_path_put_get() {
    let dir = tempdir().unwrap();
    let cfg = LsmrConfig { path: dir.path().to_path_buf(), memtable_threshold: 1, max_segments: None };
    let db = LsmrDatabase::open(cfg).await.unwrap();

    db.put_json("a".to_string(), &json!({"x":1})).await.unwrap();
    let got = db.get("a").await.unwrap();
    assert_eq!(got, Some(json!({"x":1}))); 
}

#[tokio::test]
async fn overwrite_behavior() {
    let dir = tempdir().unwrap();
    let cfg = LsmrConfig { path: dir.path().to_path_buf(), memtable_threshold: 1, max_segments: None };
    let db = LsmrDatabase::open(cfg).await.unwrap();

    db.put_json("k".to_string(), &json!({"v":1})).await.unwrap();
    db.put_json("k".to_string(), &json!({"v":2})).await.unwrap();
    let got = db.get("k").await.unwrap();
    assert_eq!(got, Some(json!({"v":2}))); 
}

#[tokio::test]
async fn deletion_and_tombstone() {
    let dir = tempdir().unwrap();
    let cfg = LsmrConfig { path: dir.path().to_path_buf(), memtable_threshold: 1, max_segments: None };
    let db = LsmrDatabase::open(cfg).await.unwrap();

    db.put_json("d".to_string(), &json!({"ok":true})).await.unwrap();
    let got = db.get("d").await.unwrap();
    assert!(got.is_some());

    db.delete("d".to_string()).await.unwrap();
    let got2 = db.get("d").await.unwrap();
    assert!(got2.is_none());
}

#[tokio::test]
async fn compaction_merges_segments() {
    let dir = tempdir().unwrap();
    let cfg = LsmrConfig { path: dir.path().to_path_buf(), memtable_threshold: 1, max_segments: None };
    let db = LsmrDatabase::open(cfg).await.unwrap();

    // create multiple segments by writing and flushing
    for i in 0..5 {
        db.put_json(format!("k{}", i), &json!({"i": i})).await.unwrap();
    }

    // trigger manual compaction
    db.compact_merge().await.unwrap();

    // ensure keys still accessible
    for i in 0..5 {
        let got = db.get(&format!("k{}", i)).await.unwrap();
        assert_eq!(got, Some(json!({"i": i}))); 
    }
}
