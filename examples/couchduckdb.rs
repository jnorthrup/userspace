use anyhow::Result;
use duckdb::{Connection, Value};
use std::sync::Arc;
use userspace::database::couch::{CouchDatabase, Document};
use tempfile::tempdir;

#[tokio::main]
async fn main() -> Result<()> {
    // create a tiny couch-style db in a temp dir and insert a couple docs
    let dir = tempdir()?;
    let dbpath = dir.path().join("couchdb");
    let db = CouchDatabase::new(dbpath).await?;

    let doc1 = Document { id: "a1".to_string(), rev: "1-aaa".to_string(), content: serde_json::json!({"name":"alice","age":30}) };
    let doc2 = Document { id: "b2".to_string(), rev: "1-bbb".to_string(), content: serde_json::json!({"name":"bob","age":40}) };

    db.put(doc1).await?;
    db.put(doc2).await?;

    // open in-memory duckdb and create a table
    let conn = Connection::open_in_memory()?;
    conn.execute("CREATE TABLE docs (id VARCHAR, rev VARCHAR, name VARCHAR, age INTEGER)", [])?;

    // read docs from CouchDatabase and insert into DuckDB
    for id in ["a1", "b2"] {
        if let Some(d) = db.get(id).await? {
            if let serde_json::Value::Object(map) = d.content {
                let name = map.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let age = map.get("age").and_then(|v| v.as_i64()).unwrap_or(0);
                conn.execute(
                    "INSERT INTO docs VALUES (?, ?, ?, ?)",
                    &[&d.id, &d.rev, &name, &age],
                )?;
            }
        }
    }

    let mut stmt = conn.prepare("SELECT id, name, age FROM docs WHERE age > ?")?;
    let mut rows = stmt.query(&[&25i64])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let age: i64 = row.get(2)?;
        println!("{}: {} ({})", id, name, age);
    }

    Ok(())
}
