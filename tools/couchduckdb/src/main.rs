use anyhow::Result;

#[cfg(feature = "duck")]
use duckdb::Connection;

fn main() -> Result<()> {
    #[cfg(feature = "duck")]
    {
        let conn = Connection::open_in_memory()?;
        conn.execute("CREATE TABLE people (id INTEGER, name VARCHAR, age INTEGER)", [])?;
        conn.execute("INSERT INTO people VALUES (1, 'alice', 30), (2, 'bob', 40)", [])?;

        let mut stmt = conn.prepare("SELECT id, name FROM people WHERE age > ?")?;
        let mut rows = stmt.query(&[&25i64])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let name: String = row.get(1)?;
            println!("{}: {}", id, name);
        }
    }

    #[cfg(not(feature = "duck"))]
    {
        println!("couchduckdb: build without 'duck' feature; enable with --features duck");
    }

    Ok(())
}
