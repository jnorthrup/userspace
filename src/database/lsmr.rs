#![allow(deprecated)]

use anyhow::Result;
use serde::{Serialize, Deserialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tokio::sync::RwLock;
use std::sync::Arc;
use std::fs::{OpenOptions, File};
use std::io::{Write, Read};
use memmap2::Mmap;
use tokio::task::spawn_blocking;
use tokio::sync::Notify;
use tokio::time::{sleep, Duration};
use chrono::Utc;

/// Configuration for the LSMR store
#[derive(Clone, Debug)]
pub struct LsmrConfig {
    /// directory to store segments
    pub path: PathBuf,
    /// memtable flush threshold in bytes
    pub memtable_threshold: usize,
    /// maximum number of retained segment files (circular retention). If None, grow forever.
    pub max_segments: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SegmentIndexEntry {
    offset: u64,
    len: u64,
}

/// Simple segment metadata persisted alongside segment data
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SegmentMeta {
    pub filename: String,
    pub index: BTreeMap<String, SegmentIndexEntry>,
    pub size: u64,
    #[serde(default = "segmeta_version_default")]
    pub version: u32,
}

fn segmeta_version_default() -> u32 { 1 }

// Current on-disk segment version written by this code path.
const SEGMENT_VERSION_WRITTEN: u32 = 2;

/// Tombstone marker stored as a special JSON object
const TOMBSTONE_MARKER: &[u8] = b"null";

/// In-memory memtable storing pending writes (doc id -> JSON bytes)
#[derive(Default)]
struct MemTable {
    map: BTreeMap<String, Vec<u8>>,
    size: usize,
}

impl MemTable {
    fn new() -> Self { Self { map: BTreeMap::new(), size: 0 } }
    fn insert(&mut self, id: String, bytes: Vec<u8>) {
        if let Some(prev) = self.map.insert(id, bytes) {
            self.size = self.size.saturating_sub(prev.len());
        }
        // recalc by adding the newly inserted value's length
        // safe because we just inserted it
        if let Some(v) = self.map.values().last() {
            self.size = self.size.saturating_add(v.len());
        }
    }
}

/// LSMR-like database: memtable -> flushed segment files + segment metadata
pub struct LsmrDatabase {
    cfg: LsmrConfig,
    memtable: RwLock<MemTable>,
    /// list of segments (newest last)
    segments: RwLock<Vec<SegmentMeta>>,
    /// notify background compactor
    compaction_notify: Arc<Notify>,
    /// background compaction handle flag (simple drop-aware)
    _bg_running: Arc<RwLock<bool>>,
}

impl LsmrDatabase {
    /// Open or create the LSMR store
    pub async fn open(cfg: LsmrConfig) -> Result<Arc<Self>> {
        tokio::fs::create_dir_all(&cfg.path).await?;

        // discover existing segment metadata files; also clean up any leftover .tmp files
        let mut segments = Vec::new();
        let mut dir = tokio::fs::read_dir(&cfg.path).await?;
        while let Some(entry) = dir.next_entry().await? {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            // cleanup temporary files left behind by interrupted flush/compaction
            if file_name.ends_with(".tmp.data") || file_name.ends_with(".tmp.meta.json") {
                let _ = std::fs::remove_file(cfg.path.join(&file_name));
                continue;
            }

            if file_name.ends_with(".meta.json") {
                let meta_path = cfg.path.join(&file_name);
                let meta: SegmentMeta = spawn_blocking(move || -> Result<SegmentMeta> {
                    let mut f = File::open(&meta_path)?;
                    let mut s = String::new();
                    f.read_to_string(&mut s)?;
                    let m: SegmentMeta = serde_json::from_str(&s)?;
                    Ok(m)
                }).await??;
                segments.push(meta);
            }
        }

        Ok(Arc::new(Self {
            cfg,
            memtable: RwLock::new(MemTable::new()),
            segments: RwLock::new(segments),
            compaction_notify: Arc::new(Notify::new()),
            _bg_running: Arc::new(RwLock::new(false)),
        }))
    }

    /// Put a document (id + JSON value) into the memtable and flush if threshold exceeded
    pub async fn put_json(&self, id: String, json: &serde_json::Value) -> Result<()> {
        let bytes = serde_json::to_vec(json)?;

        {
            let mut mt = self.memtable.write().await;
            mt.insert(id.clone(), bytes);
            if mt.size < self.cfg.memtable_threshold { return Ok(()); }
        }

        // perform flush
        self.flush_memtable().await?;
        // notify compactor that there's new data
        self.compaction_notify.notify_one();
        Ok(())
    }

    /// Delete a document by id (tombstone the key)
    pub async fn delete(&self, id: String) -> Result<()> {
        // insert tombstone into memtable
        {
            let mut mt = self.memtable.write().await;
            mt.insert(id.clone(), TOMBSTONE_MARKER.to_vec());
            if mt.size < self.cfg.memtable_threshold { return Ok(()); }
        }

        self.flush_memtable().await?;
        self.compaction_notify.notify_one();
        Ok(())
    }

    /// Read by id: search memtable first, then newest-to-oldest segments
    pub async fn get(&self, id: &str) -> Result<Option<serde_json::Value>> {
        // check memtable first
        {
            let mt = self.memtable.read().await;
            if let Some(b) = mt.map.get(id) {
                // tombstone check
                if b.as_slice() == TOMBSTONE_MARKER { return Ok(None); }
                let v = serde_json::from_slice(b)?;
                return Ok(Some(v));
            }
        }

        // check segments newest to oldest
        let segs = self.segments.read().await;
        for seg in segs.iter().rev() {
            // clone the segment metadata and index entry so we don't borrow across await
            let seg_clone = seg.clone();
            if let Some(entry) = seg_clone.index.get(id).cloned() {
                let path = self.cfg.path.join(&seg_clone.filename);
                let start = entry.offset as usize;
                let len = entry.len as usize;
                let seg_version = seg_clone.version;
                let v = spawn_blocking(move || -> Result<Option<serde_json::Value>> {
                    let f = File::open(&path)?;
                    let mmap = unsafe { Mmap::map(&f)? };
                    if seg_version >= 2 {
                        if len == 0 { return Ok(None); }
                        let slice = &mmap[start..start+len];
                        let doc = serde_json::from_slice(slice)?;
                        Ok(Some(doc))
                    } else {
                        let slice = &mmap[start..start+len];
                        if slice == TOMBSTONE_MARKER { return Ok(None); }
                        let doc = serde_json::from_slice(slice)?;
                        Ok(Some(doc))
                    }
                }).await??;
                return Ok(v);
            }
        }

        Ok(None)
    }

    /// Flush memtable into a new segment file and write metadata
    async fn flush_memtable(&self) -> Result<()> {
        // swap memtable
        let mut mt = self.memtable.write().await;
        if mt.map.is_empty() { return Ok(()); }

        let items: Vec<(String, Vec<u8>)> = mt.map.iter().map(|(k,v)| (k.clone(), v.clone())).collect();
        mt.map.clear();
        mt.size = 0;
        drop(mt);

        let path = self.cfg.path.clone();
    let seg_name = format!("segment_{:020}.data", Utc::now().timestamp_nanos());
    let meta_name = format!("{}.meta.json", seg_name);
    let seg_path = path.join(&seg_name);
    let meta_path = path.join(&meta_name);

    // temporary filenames for atomic write
    let base = seg_name.strip_suffix(".data").unwrap_or(&seg_name).to_string();
    let tmp_data_name = format!("{}.tmp.data", base);
    let tmp_meta_name = format!("{}.tmp.meta.json", base);
    let tmp_data_path = path.join(&tmp_data_name);
    let tmp_meta_path = path.join(&tmp_meta_name);

        // index is created per-segment during write
        let seg_name_cloned = seg_name.clone();
        let seg_path_cloned = seg_path.clone();
        let meta_path_cloned = meta_path.clone();
        let items_cloned = items.clone();
        // perform write to temporary files then atomically rename into place
        let seg_meta_res = spawn_blocking(move || -> Result<SegmentMeta> {
            // write data to temporary file
            let mut f = OpenOptions::new().create(true).write(true).truncate(true).open(&tmp_data_path)?;
            let mut offset = 0u64;
            let mut index = BTreeMap::new();
            for (id, bytes) in items_cloned.iter() {
                let key_bytes = id.as_bytes();
                let key_len = key_bytes.len() as u64;
                // new layout (version 2): key_len(8) | key | flag(1) | payload_len(8) | payload
                // flag: 1 => tombstone, 0 => normal
                let is_tombstone = bytes.as_slice() == TOMBSTONE_MARKER;
                let payload_len = if is_tombstone { 0u64 } else { bytes.len() as u64 };

                f.write_all(&key_len.to_le_bytes())?;
                f.write_all(key_bytes)?;
                // flag
                f.write_all(&[if is_tombstone { 1u8 } else { 0u8 }])?;
                f.write_all(&payload_len.to_le_bytes())?;
                if !is_tombstone {
                    f.write_all(bytes)?;
                }

                // index entry points to start of payload
                let entry_offset = offset + 8 + key_len + 1 + 8;
                let entry_len = payload_len;
                index.insert(id.clone(), SegmentIndexEntry { offset: entry_offset, len: entry_len });
                offset = entry_offset + entry_len;
            }
            f.flush()?;
            f.sync_all()?; // durable data

            // write metadata to temporary meta file
            let seg_meta = SegmentMeta { filename: seg_name_cloned.clone(), index, size: offset, version: SEGMENT_VERSION_WRITTEN };
            let mut mf = OpenOptions::new().create(true).write(true).truncate(true).open(&tmp_meta_path)?;
            let s = serde_json::to_string(&seg_meta)?;
            mf.write_all(s.as_bytes())?;
            mf.flush()?;
            mf.sync_all()?; // durable meta

            // atomic rename into place
            std::fs::rename(&tmp_data_path, &seg_path_cloned)?;
            std::fs::rename(&tmp_meta_path, &meta_path_cloned)?;

            // ensure directory entry is durable
            if let Some(dir) = seg_path_cloned.parent() {
                let df = OpenOptions::new().read(true).open(dir)?;
                df.sync_all()?;
            }

            Ok(seg_meta)
        }).await??;
        let meta = seg_meta_res;
        let mut segs = self.segments.write().await;
        segs.push(meta);

        // enforce circular retention if configured
        if let Some(max) = self.cfg.max_segments {
            while segs.len() > max {
                let old = segs.remove(0);
                let fname = old.filename.clone();
                // remove files (ignore errors)
                let _ = std::fs::remove_file(self.cfg.path.join(&fname));
                let _ = std::fs::remove_file(self.cfg.path.join(format!("{}.meta.json", fname)));
            }
        }

        // signal compaction as candidate (defer actual background)
        self.compaction_notify.notify_one();

        Ok(())
    }

    /// Merge a set of older segments into a single consolidated segment.
    /// This is a blocking-heavy operation so done via spawn_blocking.
    pub async fn compact_merge(&self) -> Result<()> {
        // pick older segments (all except newest) to merge if more than 1
        let mut segs = self.segments.write().await;
        let seg_count = segs.len();
        if seg_count <= 1 { return Ok(()); }

        // collect filenames to merge (all except newest)
        let merge_list: Vec<SegmentMeta> = segs.drain(0..seg_count-1).collect();
        drop(segs);

        let path = self.cfg.path.clone();
    let out_name = format!("segment_{}.data", Utc::now().timestamp_nanos());
    let out_meta_name = format!("{}.meta.json", out_name);
    let out_path = path.join(&out_name);

    // temporary filenames for atomic compaction
    let out_base = out_name.strip_suffix(".data").unwrap_or(&out_name).to_string();
    let out_tmp_data = path.join(format!("{}.tmp.data", out_base));
    let out_tmp_meta = path.join(format!("{}.tmp.meta.json", out_base));

        // perform merge on blocking thread
        let merged_meta_res = spawn_blocking(move || -> Result<SegmentMeta> {
            // write merged data to temporary output
            let mut out_f = OpenOptions::new().create(true).write(true).truncate(true).open(&out_tmp_data)?;
            let mut out_index: BTreeMap<String, SegmentIndexEntry> = BTreeMap::new();
            let mut out_offset = 0u64;

            for seg in merge_list.iter() {
                let seg_path = path.join(&seg.filename);
                let f = File::open(&seg_path)?;
                let mmap = unsafe { Mmap::map(&f)? };

                for (id, entry) in seg.index.iter() {
                    let start = entry.offset as usize;
                    let end = start + entry.len as usize;
                    let slice = &mmap[start..end];

                    // Determine whether this segment was written with version >=2 or older layout
                    // If older (version 1), the payload may equal TOMBSTONE_MARKER when tombstone.
                    let is_old_tombstone = seg.version <= 1 && slice == TOMBSTONE_MARKER;
                    if is_old_tombstone { continue; }

                    // For version 2, tombstones are represented with payload_len == 0 and were
                    // written with a preceding flag; the index entry will have len == 0.
                    if seg.version >= 2 && entry.len == 0 { continue; }

                    // write key and payload with new layout (flag + payload_len)
                    let key_bytes = id.as_bytes();
                    let key_len = key_bytes.len() as u64;
                    let payload_len = slice.len() as u64;
                    out_f.write_all(&key_len.to_le_bytes())?;
                    out_f.write_all(key_bytes)?;
                    out_f.write_all(&[0u8])?; // flag 0 - normal
                    out_f.write_all(&payload_len.to_le_bytes())?;
                    out_f.write_all(slice)?;

                    let entry_offset = out_offset + 8 + key_len + 1 + 8;
                    out_index.insert(id.clone(), SegmentIndexEntry { offset: entry_offset, len: payload_len });
                    out_offset = entry_offset + payload_len;
                }
                out_f.flush()?;

                // delete old segment files
                let _ = std::fs::remove_file(path.join(&seg.filename));
                let _ = std::fs::remove_file(path.join(format!("{}.meta.json", seg.filename)));
            }

            out_f.sync_all()?;

            let seg_meta = SegmentMeta { filename: out_name.clone(), index: out_index, size: out_offset, version: SEGMENT_VERSION_WRITTEN };
            let mut mf = OpenOptions::new().create(true).write(true).truncate(true).open(&out_tmp_meta)?;
            let s = serde_json::to_string(&seg_meta)?;
            mf.write_all(s.as_bytes())?;
            mf.flush()?;
            mf.sync_all()?;

            // atomic rename into final names
            std::fs::rename(&out_tmp_data, &out_path)?;
            std::fs::rename(&out_tmp_meta, path.join(&out_meta_name))?;

            if let Some(dir) = out_path.parent() {
                let df = OpenOptions::new().read(true).open(dir)?;
                df.sync_all()?;
            }

            Ok(seg_meta)
        }).await??;

        // spawn_blocking returned Result<SegmentMeta>
        let merged_meta = merged_meta_res;

    // insert merged meta before the newest segment so newest remains last
    let mut segs = self.segments.write().await;
    segs.insert(0, merged_meta);
        Ok(())
    }

    /// Start a simple background compaction task that waits for notifications.
    /// It's safe to call multiple times; only one worker will run.
    pub async fn start_background_compactor(self: &Arc<Self>) {
        let flag = self._bg_running.clone();
        let mut running = flag.write().await;
        if *running { return; }
        *running = true;
        drop(running);

        let me = self.clone();
        let notify = self.compaction_notify.clone();
        tokio::spawn(async move {
            loop {
                // wait for notification or timeout
                tokio::select! {
                    _ = notify.notified() => {},
                    _ = sleep(Duration::from_secs(5)) => {},
                }

                // try to compact; ignore errors
                let _ = me.compact_merge().await;

                // small sleep to avoid busy loop
                sleep(Duration::from_millis(100)).await;
            }
        });
    }
}

