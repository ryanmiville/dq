use std::{
    fs,
    io::{self, IsTerminal},
    path::Path,
};

use crate::{
    duckbox::DuckBox,
    format::Format,
    plan::{Op, Plan},
};
use anyhow::{Context, Result};
use duckdb::Connection;
use tempfile::{Builder, TempDir};

pub fn to(conn: &Connection, format: &Format) -> Result<()> {
    let plan = Plan::read_from(io::stdin().lock()).context("failed to read input plan")?;
    let query = plan.compile_sql();
    let sql = format!("COPY ({query}) TO {};", format.copy_format());
    finish_execution(
        &plan,
        conn.execute_batch(&sql)
            .context("failed to convert plan to output format"),
    )
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
    let plan = Plan::read_from(io::stdin().lock())
        .context("failed to read input plan")?
        .with_op(Op::Where {
            clause: clause.to_string(),
        });
    emit_plan_or_pretty(conn, &plan).context("failed to filter data")
}

pub fn limit(conn: &Connection, count: &str) -> Result<()> {
    let plan = Plan::read_from(io::stdin().lock())
        .context("failed to read input plan")?
        .with_op(Op::Limit {
            count: count.to_string(),
        });
    emit_plan_or_pretty(conn, &plan).context("failed to limit rows")
}

pub fn order_by(conn: &Connection, clause: &str) -> Result<()> {
    let plan = Plan::read_from(io::stdin().lock())
        .context("failed to read input plan")?
        .with_op(Op::OrderBy {
            clause: clause.to_string(),
        });
    emit_plan_or_pretty(conn, &plan).context("failed to order rows")
}

pub fn describe(conn: &Connection) -> Result<()> {
    let plan = Plan::read_from(io::stdin().lock())
        .context("failed to read input plan")?
        .with_op(Op::Describe);
    emit_plan_or_pretty(conn, &plan).context("failed to describe input")
}

pub fn summarize(conn: &Connection) -> Result<()> {
    let plan = Plan::read_from(io::stdin().lock())
        .context("failed to read input plan")?
        .with_op(Op::Summarize);
    emit_plan_or_pretty(conn, &plan).context("failed to summarize input")
}

fn emit_plan_or_pretty(conn: &Connection, plan: &Plan) -> Result<()> {
    if stdout_is_terminal() {
        return finish_execution(plan, print_pretty_query(conn, &plan.compile_sql()));
    }

    plan.write_to(io::stdout().lock())
        .context("failed to write output plan")
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

fn stdout_is_terminal() -> bool {
    io::stdout().is_terminal()
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
    let temp_dir = create_temp_source_dir()?;
    let temp_path = temp_dir.path().join("source.parquet");
    let sql = format!(
        "COPY (SELECT * FROM {}) TO {} (FORMAT PARQUET);",
        format.read_fn(),
        sql_string_literal(temp_path.to_string_lossy().as_ref())
    );
    conn.execute_batch(&sql)
        .context("failed to materialize input to temp parquet")?;
    fs::write(temp_dir.path().join(".dq-owned"), b"")
        .context("failed to mark temp source dir")?;

    let persisted_dir = temp_dir.keep();
    let absolute = fs::canonicalize(persisted_dir.join("source.parquet")).with_context(|| {
        format!(
            "failed to resolve temp parquet path `{}`",
            persisted_dir.join("source.parquet").display()
        )
    })?;
    Ok(absolute.to_string_lossy().into_owned())
}

fn create_temp_source_dir() -> Result<TempDir> {
    Builder::new()
        .prefix("dq-")
        .tempdir()
        .context("failed to create temp source dir")
}

fn finish_execution(plan: &Plan, execution: Result<()>) -> Result<()> {
    let cleanup = cleanup_owned_source(plan);
    match (execution, cleanup) {
        (Err(error), _) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(error)) => Err(error),
    }
}

fn cleanup_owned_source(plan: &Plan) -> Result<()> {
    let Some(source_dir) = Path::new(&plan.source_path).parent() else {
        return Ok(());
    };
    let marker_path = source_dir.join(".dq-owned");
    if !marker_path
        .try_exists()
        .with_context(|| format!("failed to inspect cleanup marker `{}`", marker_path.display()))?
    {
        return Ok(());
    }

    fs::remove_dir_all(source_dir)
        .with_context(|| format!("failed to clean up temp source dir `{}`", source_dir.display()))
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
