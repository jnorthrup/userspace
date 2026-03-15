use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::Result;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use memmap2::Mmap;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write, Read};
use std::sync::Arc;

/// CouchDB-like document representation (stored as JSON blob in data file)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub rev: String,
    pub content: serde_json::Value,
}

/// Small index entry for ISAM-style layout
#[derive(Clone, Debug, Serialize, Deserialize)]
struct IndexEntry {
    offset: u64,
    len: u64,
    rev: String,
}

/// Represents a CouchDB-like ISAM-backed database using an append-only data file
/// plus a JSON index mapping doc id -> IndexEntry. Reads are served via mmap
/// for efficient zero-copy deserialization.
pub struct CouchDatabase {
    /// Root path for database files
    pub path: PathBuf,

    /// In-memory index (persisted to `index.json`)
    index: RwLock<HashMap<String, IndexEntry>>,

    /// data file name (append-only)
    data_file: PathBuf,
    index_file: PathBuf,
}

impl CouchDatabase {
    /// Create or open an existing database directory and load the index.
    pub async fn new(path: PathBuf) -> Result<Self> {
        tokio::fs::create_dir_all(&path).await?;

        let data_file = path.join("data.bin");
        let index_file = path.join("index.json");

        // load index if present (blocking file I/O)
        let idx: HashMap<String, IndexEntry> = if index_file.exists() {
            let index_file2 = index_file.clone();
            tokio::task::spawn_blocking(move || -> Result<HashMap<String, IndexEntry>> {
                let mut f = File::open(&index_file2)?;
                let mut s = String::new();
                f.read_to_string(&mut s)?;
                let m: HashMap<String, IndexEntry> = serde_json::from_str(&s)?;
                Ok(m)
            }).await??
        } else {
            HashMap::new()
        };

        Ok(Self {
            path,
            index: RwLock::new(idx),
            data_file,
            index_file,
        })
    }

    /// Insert or update a document. Appends JSON bytes to the data file and
    /// updates the index atomically (in-memory then persisted to index.json).
    pub async fn put(&self, doc: Document) -> Result<()> {
        let bytes = serde_json::to_vec(&doc)?;
        let len = bytes.len() as u64;

        // perform blocking append in spawn_blocking to avoid blocking async runtime
        let data_path = self.data_file.clone();
        let index_path = self.index_file.clone();
        let id = doc.id.clone();
        let rev = doc.rev.clone();

        // append and get offset
        let offset = tokio::task::spawn_blocking(move || -> Result<u64> {
            let mut f = OpenOptions::new().create(true).append(true).read(true).open(&data_path)?;
            // seek to end to get offset
            let offset = f.seek(SeekFrom::End(0))?;
            f.write_all(&bytes)?;
            f.flush()?;
            Ok(offset)
        }).await??;

        // update in-memory index and persist
        let mut idx = self.index.write().await;
        idx.insert(id.clone(), IndexEntry { offset, len, rev });

        // persist index (blocking)
        let idx_clone = idx.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut f = OpenOptions::new().create(true).write(true).truncate(true).open(&index_path)?;
            let s = serde_json::to_string(&idx_clone)?;
            f.write_all(s.as_bytes())?;
            f.flush()?;
            Ok(())
        }).await??;

        Ok(())
    }

    /// Retrieve a document by ID using memory-mapped reads.
    pub async fn get(&self, id: &str) -> Result<Option<Document>> {
        // check index in-memory
        let idx = self.index.read().await;
        let entry = match idx.get(id) {
            Some(e) => e.clone(),
            None => return Ok(None),
        };
        drop(idx);

        let data_path = self.data_file.clone();
        // perform blocking mmap and deserialize in spawn_blocking
        let doc = tokio::task::spawn_blocking(move || -> Result<Document> {
            let f = File::open(&data_path)?;
            let mmap = unsafe { Mmap::map(&f)? };
            let start = entry.offset as usize;
            let end = start + entry.len as usize;
            if end > mmap.len() {
                anyhow::bail!("index entry out of bounds");
            }
            let slice = &mmap[start..end];
            let doc: Document = serde_json::from_slice(slice)?;
            Ok(doc)
        }).await?;

        Ok(Some(doc?))
    }

    /// Delete a document by ID (logical delete: remove from index and persist).
    /// Data remains in the append-only file (tombstones could be added later).
    pub async fn delete(&self, id: &str) -> Result<()> {
        let mut idx = self.index.write().await;
        if idx.remove(id).is_some() {
            let idx_clone = idx.clone();
            let index_path = self.index_file.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let mut f = OpenOptions::new().create(true).write(true).truncate(true).open(&index_path)?;
                let s = serde_json::to_string(&idx_clone)?;
                f.write_all(s.as_bytes())?;
                f.flush()?;
                Ok(())
            }).await??;
        }
        Ok(())
    }
}