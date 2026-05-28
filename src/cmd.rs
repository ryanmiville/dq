use std::{
    env, fs,
    io::{self, IsTerminal},
    path::Path,
};

use crate::{
    format::Format,
    plan::{Op, Plan},
};
use anyhow::{Context, Result};
use duckdb::{Connection, DuckboxColorMode, DuckboxMaximumWidth, DuckboxOptions, DuckboxRowLimit};
use tempfile::{Builder, TempDir};
use terminal_size::{Width, terminal_size_of};

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
    print!("{table}");
    Ok(())
}

fn pretty_query(conn: &Connection, query: &str) -> Result<String> {
    conn.query_duckbox_with_options(query, [], &duckbox_options())
        .context("failed to format pretty output")
}

fn duckbox_options() -> DuckboxOptions {
    DuckboxOptions::default()
        .with_maximum_width_limit(maximum_width())
        .with_maximum_row_limit(row_limit("DQ_MAX_ROWS", 20))
        .with_color_mode(color_mode())
}

fn maximum_width() -> DuckboxMaximumWidth {
    maximum_width_from_env(
        parse_env_u64("DQ_MAX_WIDTH"),
        terminal_size_of(io::stdout()).map(|(Width(width), _)| u64::from(width)),
    )
}

fn maximum_width_from_env(
    configured: Option<u64>,
    terminal_width: Option<u64>,
) -> DuckboxMaximumWidth {
    DuckboxMaximumWidth::Cells(match configured {
        Some(width) if width > 0 => width,
        _ => terminal_width.map_or(120, |width| width.min(120)),
    })
}

fn row_limit(var_name: &str, default: u64) -> DuckboxRowLimit {
    match parse_env_u64(var_name).unwrap_or(default) {
        0 => DuckboxRowLimit::Unlimited,
        rows => DuckboxRowLimit::Rows(rows),
    }
}

fn color_mode() -> DuckboxColorMode {
    if stdout_is_terminal() && env::var_os("NO_COLOR").is_none() {
        DuckboxColorMode::Always
    } else {
        DuckboxColorMode::Never
    }
}

fn parse_env_u64(var_name: &str) -> Option<u64> {
    env::var(var_name).ok().and_then(|s| s.parse().ok())
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
    fs::write(temp_dir.path().join(".dq-owned"), b"").context("failed to mark temp source dir")?;

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
    if !marker_path.try_exists().with_context(|| {
        format!(
            "failed to inspect cleanup marker `{}`",
            marker_path.display()
        )
    })? {
        return Ok(());
    }

    fs::remove_dir_all(source_dir).with_context(|| {
        format!(
            "failed to clean up temp source dir `{}`",
            source_dir.display()
        )
    })
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_duckbox_width_caps_large_terminals() {
        assert_eq!(
            maximum_width_from_env(None, Some(160)),
            DuckboxMaximumWidth::Cells(120)
        );
    }

    #[test]
    fn default_duckbox_width_uses_small_terminal_width() {
        assert_eq!(
            maximum_width_from_env(None, Some(100)),
            DuckboxMaximumWidth::Cells(100)
        );
    }

    #[test]
    fn default_duckbox_width_falls_back_without_terminal_width() {
        assert_eq!(
            maximum_width_from_env(None, None),
            DuckboxMaximumWidth::Cells(120)
        );
    }

    #[test]
    fn zero_duckbox_width_uses_default_width() {
        assert_eq!(
            maximum_width_from_env(Some(0), Some(100)),
            DuckboxMaximumWidth::Cells(100)
        );
    }

    #[test]
    fn configured_duckbox_width_wins() {
        assert_eq!(
            maximum_width_from_env(Some(132), Some(100)),
            DuckboxMaximumWidth::Cells(132)
        );
    }
}
