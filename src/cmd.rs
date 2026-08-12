use std::{
    env, fs,
    io::{self, IsTerminal, Write},
    path::Path,
};

use crate::{
    format::{InputFormat, OutputExecution, OutputFormat},
    plan::{Op, Plan},
    stream::{
        duplicate_stdin, finish_stdin_payload, is_broken_pipe, prepare_stdin_payload,
        read_plan_header, write_plan_and_payload,
    },
};
use anyhow::{Context, Result};
use duckdb::{Connection, DuckboxColorMode, DuckboxMaximumWidth, DuckboxOptions, DuckboxRowLimit};
use terminal_size::{Width, terminal_size_of};

pub fn to(conn: &Connection, format: &OutputFormat) -> Result<()> {
    let mut input = duplicate_stdin()?;
    let plan = read_plan_header(&mut input).context("failed to read input plan")?;
    let payload_thread = if plan.source.is_stream() {
        prepare_stdin_payload(input)?
    } else {
        drop(input);
        None
    };

    let execution = execute_to(conn, &plan, format);
    finish_stdin_payload(payload_thread);
    match execution {
        Err(error) if is_broken_pipe(&error) => Ok(()),
        result => result,
    }
}

fn execute_to(conn: &Connection, plan: &Plan, format: &OutputFormat) -> Result<()> {
    match format.execution() {
        OutputExecution::Copy(destination) => execute_copy(conn, plan, &destination),
        OutputExecution::Pretty => print_pretty_query(conn, &plan.compile_sql()),
    }
}

fn execute_copy(conn: &Connection, plan: &Plan, destination: &str) -> Result<()> {
    let source_query = Plan {
        ops: Vec::new(),
        ..plan.clone()
    }
    .compile_sql();
    let query = plan.compile_sql_from_table("dq_source");
    conn.execute_batch(&format!(
        "CREATE TEMP TABLE dq_source AS {source_query}; CREATE TEMP TABLE dq_result AS {query}; COPY dq_result TO {destination};"
    ))
    .context("failed to convert plan to output format")
}

pub fn from(conn: &Connection, format: &InputFormat) -> Result<()> {
    let plan = build_source_plan(format).context("failed to build input plan")?;
    if stdout_is_terminal() {
        print_pretty_query(conn, &plan.compile_sql()).context("failed to read input")
    } else if plan.source.is_stream() {
        write_plan_and_payload(&plan, Some(duplicate_stdin()?), io::stdout().lock())
            .context("failed to read input")
    } else {
        write_plan_and_payload(&plan, None::<std::fs::File>, io::stdout().lock())
            .context("failed to read input")
    }
}

pub fn select(conn: &Connection, columns: &str) -> Result<()> {
    transform(
        conn,
        Op::Select {
            columns: columns.to_string(),
        },
        "failed to select columns",
    )
}

pub fn where_clause(conn: &Connection, clause: &str) -> Result<()> {
    transform(
        conn,
        Op::Where {
            clause: clause.to_string(),
        },
        "failed to filter data",
    )
}

pub fn limit(conn: &Connection, count: &str) -> Result<()> {
    transform(
        conn,
        Op::Limit {
            count: count.to_string(),
        },
        "failed to limit rows",
    )
}

pub fn offset(conn: &Connection, count: &str) -> Result<()> {
    transform(
        conn,
        Op::Offset {
            count: count.to_string(),
        },
        "failed to offset rows",
    )
}

pub fn order_by(conn: &Connection, clause: &str) -> Result<()> {
    transform(
        conn,
        Op::OrderBy {
            clause: clause.to_string(),
        },
        "failed to order rows",
    )
}

pub fn describe(conn: &Connection) -> Result<()> {
    transform(conn, Op::Describe, "failed to describe input")
}

pub fn summarize(conn: &Connection) -> Result<()> {
    transform(conn, Op::Summarize, "failed to summarize input")
}

fn transform(conn: &Connection, op: Op, context: &'static str) -> Result<()> {
    let mut input = duplicate_stdin()?;
    let plan = read_plan_header(&mut input)
        .context("failed to read input plan")?
        .with_op(op);

    if stdout_is_terminal() {
        let payload_thread = if plan.source.is_stream() {
            prepare_stdin_payload(input)?
        } else {
            drop(input);
            None
        };
        let execution = print_pretty_query(conn, &plan.compile_sql()).context(context);
        finish_stdin_payload(payload_thread);
        match execution {
            Err(error) if is_broken_pipe(&error) => Ok(()),
            result => result,
        }
    } else {
        write_plan_and_payload(&plan, Some(input), io::stdout().lock()).context(context)
    }
}

fn print_pretty_query(conn: &Connection, query: &str) -> Result<()> {
    let table = pretty_query(conn, query)?;
    io::stdout()
        .lock()
        .write_all(table.as_bytes())
        .context("failed to write pretty output")
}

fn pretty_query(conn: &Connection, query: &str) -> Result<String> {
    conn.query_duckbox_with_options(query, &duckbox_options())
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

fn build_source_plan(format: &InputFormat) -> Result<Plan> {
    match format {
        InputFormat::Path(path) => Ok(Plan::from_path(resolve_existing_path(path)?)),
        _ => Ok(Plan::from_stream(format.read_fn())),
    }
}

fn resolve_existing_path(path: &str) -> Result<String> {
    let absolute = fs::canonicalize(Path::new(path))
        .with_context(|| format!("failed to resolve input path `{path}`"))?;
    Ok(absolute.to_string_lossy().into_owned())
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
