use std::fmt::Display;

use crate::format::{Format, Preset};
use anyhow::{Context, Result, bail};
use duckdb::{Connection, arrow::util::pretty::pretty_format_batches};

pub fn to(conn: &Connection, format: &Format) -> Result<()> {
    if is_pretty(format) {
        let table = to_pretty(conn)?;
        println!("{table}");
        return Ok(());
    }

    let sql = format!(
        "CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin'); COPY dq_input TO '/dev/stdout' {};",
        format.copy_format()
    );
    conn.execute_batch(&sql)
        .context("failed to convert stdin to output format")
}

pub fn from(conn: &Connection, format: &Format) -> Result<()> {
    if is_pretty(format) {
        bail!("pretty is only supported by `dq to pretty`");
    }

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

fn is_pretty(format: &Format) -> bool {
    matches!(format, Format::Preset(Preset::Pretty))
}

fn to_pretty(conn: &Connection) -> Result<impl Display> {
    conn.execute_batch("CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin');")
        .context("failed to read arrow input for pretty output")?;

    let mut stmt = conn
        .prepare("SELECT * FROM dq_input")
        .context("failed to prepare pretty output query")?;
    let batches: Vec<_> = stmt
        .query_arrow([])
        .context("failed to query pretty output")?
        .collect();
    pretty_format_batches(&batches).context("failed to format pretty output")
}
