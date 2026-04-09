use std::{
    fs,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    duckbox::DuckBox,
    format::Format,
    plan::{Op, Plan},
};
use anyhow::{Context, Result};
use duckdb::Connection;

pub fn to(conn: &Connection, format: &Format) -> Result<()> {
    let plan = Plan::read_from(io::stdin().lock()).context("failed to read input plan")?;
    let query = plan.compile_sql();
    let sql = format!("COPY ({query}) TO {};", format.copy_format());
    conn.execute_batch(&sql)
        .context("failed to convert plan to output format")
}

pub fn from(conn: &Connection, format: &Format) -> Result<()> {
    let plan = build_source_plan(conn, format).context("failed to build input plan")?;
    emit_plan_or_pretty(conn, &plan).context("failed to read input")
}

pub fn select(conn: &Connection, columns: &str) -> Result<()> {
    let plan = Plan::read_from(io::stdin().lock())
        .context("failed to read input plan")?
        .with_op(Op::Select {
            columns: columns.to_string(),
        });
    emit_plan_or_pretty(conn, &plan).context("failed to select columns")
}

pub fn where_clause(conn: &Connection, clause: &str) -> Result<()> {
    let query = format!("SELECT * FROM read_arrow('/dev/stdin') WHERE {clause}");
    emit_relation_query(conn, &query).context("failed to filter data")
}

pub fn limit(conn: &Connection, count: &str) -> Result<()> {
    let query = format!("SELECT * FROM read_arrow('/dev/stdin') LIMIT {count}");
    emit_relation_query(conn, &query).context("failed to limit rows")
}

pub fn order_by(conn: &Connection, clause: &str) -> Result<()> {
    let query = format!("SELECT * FROM read_arrow('/dev/stdin') ORDER BY {clause}");
    emit_relation_query(conn, &query).context("failed to order rows")
}

pub fn describe(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin');")
        .context("failed to describe input")?;

    let query = "SELECT * FROM (DESCRIBE dq_input)";
    emit_relation_query(conn, query).context("failed to describe input")
}

pub fn summarize(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin');")
        .context("failed to summarize input")?;

    let query = "SELECT * FROM (SUMMARIZE dq_input)";
    emit_relation_query(conn, query).context("failed to summarize input")
}

fn emit_relation_query(conn: &Connection, query: &str) -> Result<()> {
    match output_mode() {
        OutputMode::Arrow => emit_arrow_query(conn, query),
        OutputMode::Pretty => print_pretty_query(conn, query),
    }
}

fn emit_plan_or_pretty(conn: &Connection, plan: &Plan) -> Result<()> {
    if stdout_is_terminal() {
        return print_pretty_query(conn, &plan.compile_sql());
    }

    plan.write_to(io::stdout().lock())
        .context("failed to write output plan")
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
    let batches = stmt
        .query_arrow([])
        .context("failed to query pretty output")?;
    DuckBox::default()
        .render_streaming(batches)
        .context("failed to format pretty output")
}

fn output_mode() -> OutputMode {
    if stdout_is_terminal() {
        OutputMode::Pretty
    } else {
        OutputMode::Arrow
    }
}

fn stdout_is_terminal() -> bool {
    io::stdout().is_terminal()
}

enum OutputMode {
    Arrow,
    Pretty,
}

fn build_source_plan(conn: &Connection, format: &Format) -> Result<Plan> {
    match format {
        Format::Path(path) => Ok(Plan::new(resolve_existing_path(path)?)),
        _ => Ok(Plan::new(materialize_input_to_temp_parquet(conn, format)?)),
    }
}

fn resolve_existing_path(path: &str) -> Result<String> {
    let absolute = fs::canonicalize(Path::new(path))
        .with_context(|| format!("failed to resolve input path `{path}`"))?;
    Ok(absolute.to_string_lossy().into_owned())
}

fn materialize_input_to_temp_parquet(conn: &Connection, format: &Format) -> Result<String> {
    let temp_path = next_temp_parquet_path();
    let sql = format!(
        "COPY (SELECT * FROM {}) TO {} (FORMAT PARQUET);",
        format.read_fn(),
        sql_string_literal(temp_path.to_string_lossy().as_ref())
    );
    conn.execute_batch(&sql)
        .context("failed to materialize input to temp parquet")?;

    let absolute = fs::canonicalize(&temp_path)
        .with_context(|| format!("failed to resolve temp parquet path `{}`", temp_path.display()))?;
    Ok(absolute.to_string_lossy().into_owned())
}

fn next_temp_parquet_path() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("dq-{}-{timestamp}.parquet", std::process::id()))
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
