use crate::format::Format;
use anyhow::{Context, Result};
use duckdb::Connection;

pub fn to(conn: &Connection, format: &Format) -> Result<()> {
    let sql = format!(
        "CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin'); COPY dq_input TO '/dev/stdout' {};",
        format.copy_format()
    );
    conn.execute_batch(&sql)
        .context("failed to convert stdin to output format")
}

pub fn from(conn: &Connection, format: &Format) -> Result<()> {
    let sql = format!(
        "COPY (SELECT * FROM {}) TO '/dev/stdout' (FORMAT ARROW);",
        format.read_fn()
    );
    conn.execute_batch(&sql)
        .context("failed to convert stdin to arrow")
}

pub fn select(conn: &Connection, columns: &str) -> Result<()> {
    let sql = format!(
        "CREATE TEMP TABLE dq_input AS SELECT {columns} FROM read_arrow('/dev/stdin'); COPY dq_input TO '/dev/stdout' (FORMAT ARROW);"
    );
    conn.execute_batch(&sql).context("failed to select columns")
}

pub fn where_clause(conn: &Connection, clause: &str) -> Result<()> {
    let sql = format!(
        "CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin') WHERE {clause}; COPY dq_input TO '/dev/stdout' (FORMAT ARROW);"
    );
    conn.execute_batch(&sql).context("failed to filter data")
}
