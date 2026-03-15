oroboros - minimal filewatcher -> CouchDB attachment batch uploader

Overview
--------
This small Rust CLI watches a directory and, when files change, batches changes and uploads them as attachments to CouchDB 1.7.2 documents. It's intentionally minimal and designed to be a practical starting point.

Path convention
---------------
Files should be placed under: <watch-dir>/<docid>/<attachment-name>

Example:

watch_dir/alice/profile.png -> uploads as attachment name `profile.png` to document `alice` in the configured DB.

Build
-----
From the repository root:

    cd tools/oroboros
    cargo build --release

Run
---
Basic invocation:

    ./target/release/oroboros --watch /path/to/watch --url http://127.0.0.1:5984 --db mydb

If your CouchDB requires basic auth:

    ./target/release/oroboros --watch /path/to/watch --url http://host:5984 --db mydb --user admin --pass secret

Options
-------
--watch <dir>      Directory to watch (default: .)
--url <url>        CouchDB base URL (default: http://127.0.0.1:5984)
--db <name>        Database name (default: mydb)
--user <user>      Basic auth user
--pass <pass>      Basic auth password
--interval <ms>    Batch flush interval in milliseconds (default: 2000)

Notes & limitations
-------------------
- Expects CouchDB documents to already exist; it fetches the document to obtain _rev before attaching. If the doc does not exist, attachments will fail. You can extend it to create the doc first.
- No retries/backoff for simplicity.
- No TLS certificate validation customization beyond reqwest defaults.
- Uses a simple path-based mapping of doc id -> attachment name; adjust to your needs.

License
-------
Minimal example code provided as-is. Modify as needed for your project.
