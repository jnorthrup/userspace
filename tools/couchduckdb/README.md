
# couchduckdb

This small example demonstrates embedding DuckDB, but the DuckDB dependency is optional and gated behind the Cargo feature `duck`.

Default behavior

- Building/running this crate without features will compile quickly and print a short message:

  cargo run

Enable DuckDB

- To compile and run the DuckDB example (this will pull Arrow and related crates):

  cargo run --features duck

Notes

- The `duck` feature enables the `duckdb` crate and can pull heavy Arrow-related dependencies. On some systems or toolchains this may trigger compilation issues in transitive crates (for example, an ambiguity in `arrow-arith` was observed during development). If you hit a compile error when enabling the feature, keep the crate feature off and run the example in an environment where Arrow builds successfully, or consider using a separate lightweight binary that depends on `duckdb` only when needed.

