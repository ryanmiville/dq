use anyhow::{Context, Result};
use duckdb::Connection;

pub fn where_clause(connection: &Connection, clause: &str) -> Result<()> {
    let sql = format!(
        "CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin') WHERE {clause}; COPY dq_input TO '/dev/stdout' (FORMAT ARROW);"
    );

    connection
        .execute_batch(&sql)
        .context("failed to select columns")
}
