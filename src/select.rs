use anyhow::{Context, Result};
use duckdb::Connection;

pub fn select(connection: &Connection, columns: &str) -> Result<()> {
    let sql = format!(
        "CREATE TEMP TABLE dq_input AS SELECT {columns} FROM read_arrow('/dev/stdin'); COPY dq_input TO '/dev/stdout' (FORMAT ARROW);"
    );
    connection
        .execute_batch(&sql)
        .context("failed to select columns")
}
