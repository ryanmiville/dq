use std::io::{self, IsTerminal};

use crate::{
    duckbox::{Config as DuckBoxConfig, DuckBox},
    format::{Format, Preset},
};
use anyhow::{Context, Result, bail};
use duckdb::Connection;

pub fn to(conn: &Connection, format: &Format) -> Result<()> {
    if is_pretty(format) {
        print_pretty_query(conn, "SELECT * FROM read_arrow('/dev/stdin')")?;
        return Ok(());
    }

    let sql = format!(
        "CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin'); COPY dq_input TO {};",
        format.copy_format()
    );
    conn.execute_batch(&sql)
        .context("failed to convert stdin to output format")
}

pub fn from(conn: &Connection, format: &Format) -> Result<()> {
    if is_pretty(format) {
        bail!("pretty is only supported by `dq to pretty`");
    }

    let query = format!("SELECT * FROM {}", format.read_fn());
    emit_relation_query(conn, &query).context("failed to read input")
}

pub fn select(conn: &Connection, columns: &str) -> Result<()> {
    let query = format!("SELECT {columns} FROM read_arrow('/dev/stdin')");
    emit_relation_query(conn, &query).context("failed to select columns")
}

pub fn where_clause(conn: &Connection, clause: &str) -> Result<()> {
    let query = format!("SELECT * FROM read_arrow('/dev/stdin') WHERE {clause}");
    emit_relation_query(conn, &query).context("failed to filter data")
}

fn emit_relation_query(conn: &Connection, query: &str) -> Result<()> {
    match output_mode() {
        OutputMode::Arrow => emit_arrow_query(conn, query),
        OutputMode::Pretty => print_pretty_query(conn, query),
    }
}

fn emit_arrow_query(conn: &Connection, query: &str) -> Result<()> {
    let sql = format!("COPY ({query}) TO '/dev/stdout' (FORMAT ARROW);");
    conn.execute_batch(&sql)
        .context("failed to write arrow output")
}

fn print_pretty_query(conn: &Connection, query: &str) -> Result<()> {
    let table = pretty_query(conn, query)?;
    println!("{table}");
    Ok(())
}

fn pretty_query(conn: &Connection, query: &str) -> Result<String> {
    let sql = format!("CREATE TEMP TABLE dq_output AS {query};");
    conn.execute_batch(&sql)
        .context("failed to materialize pretty output")?;

    let mut stmt = conn
        .prepare("SELECT * FROM dq_output")
        .context("failed to prepare pretty output query")?;
    let batches: Vec<_> = stmt
        .query_arrow([])
        .context("failed to query pretty output")?
        .collect();
    DuckBox::new(DuckBoxConfig::default())
        .render(&batches)
        .context("failed to format pretty output")
}

fn is_pretty(format: &Format) -> bool {
    matches!(format, Format::Preset(Preset::Pretty))
}

fn output_mode() -> OutputMode {
    if io::stdout().is_terminal() {
        OutputMode::Pretty
    } else {
        OutputMode::Arrow
    }
}

enum OutputMode {
    Arrow,
    Pretty,
}
