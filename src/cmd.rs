use std::{
    env, fs,
    io::{self, IsTerminal, Write},
    path::Path,
};

use crate::{
    format::{InputFormat, OutputExecution, OutputFormat},
    plan::{Op, Plan},
    storage,
    stream::{
        duplicate_stdin, finish_stdin_payload, is_broken_pipe, prepare_stdin_payload,
        read_plan_header, write_plan_and_payload,
    },
};
use anyhow::{Context, Result};
use duckdb::{Connection, DuckboxColorMode, DuckboxMaximumWidth, DuckboxOptions, DuckboxRowLimit};
use terminal_size::{Width, terminal_size_of};

pub fn to(format: &OutputFormat) -> Result<()> {
    let mut input = duplicate_stdin()?;
    let plan = read_plan_header(&mut input).context("failed to read input plan")?;
    let payload_thread = if plan.source.is_stream() {
        prepare_stdin_payload(input)?
    } else {
        drop(input);
        None
    };

    let execution = open_connection(&plan).and_then(|conn| execute_to(&conn, &plan, format));
    finish_stdin_payload(payload_thread);
    match execution {
        Err(error) if is_broken_pipe(&error) => Ok(()),
        result => result,
    }
}

pub fn sql() -> Result<()> {
    let mut input = duplicate_stdin()?;
    let plan = read_plan_header(&mut input).context("failed to read input plan")?;
    let output = writeln!(io::stdout().lock(), "{};", plan.compile_sql())
        .context("failed to write compiled sql");

    io::copy(&mut input, &mut io::sink()).context("failed to drain input payload")?;
    match output {
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
    conn.execute_batch(&copy_query(plan, destination))
        .context("failed to convert plan to output format")
}

fn copy_query(plan: &Plan, destination: &str) -> String {
    if plan.source.is_stream() && plan.ops.iter().any(|op| matches!(op, Op::Summarize)) {
        return stream_summarize_copy_query(plan, destination);
    }

    format!(
        "CREATE TEMP TABLE dq_result AS {}; COPY dq_result TO {destination};",
        plan.compile_sql()
    )
}

fn stream_summarize_copy_query(plan: &Plan, destination: &str) -> String {
    // DuckDB binds SUMMARIZE twice under CTAS. A non-seekable source is exhausted by
    // the first bind, so preserve the existing stream behavior for this one case.
    let source_query = Plan {
        ops: Vec::new(),
        ..plan.clone()
    }
    .compile_sql();
    let result_query = plan.compile_sql_from_table("dq_input");
    format!(
        "CREATE TEMP TABLE dq_input AS {source_query}; CREATE TEMP TABLE dq_result AS {result_query}; COPY dq_result TO {destination};"
    )
}

pub fn from(format: &InputFormat) -> Result<()> {
    let plan = build_source_plan(format).context("failed to build input plan")?;
    if stdout_is_terminal() {
        let conn = open_connection(&plan)?;
        print_pretty_query(&conn, &plan.compile_sql()).context("failed to read input")
    } else if plan.source.is_stream() {
        write_plan_and_payload(&plan, Some(duplicate_stdin()?), io::stdout().lock())
            .context("failed to read input")
    } else {
        write_plan_and_payload(&plan, None::<std::fs::File>, io::stdout().lock())
            .context("failed to read input")
    }
}

pub fn select(columns: &str) -> Result<()> {
    transform(
        Op::Select {
            columns: columns.to_string(),
        },
        "failed to select columns",
    )
}

pub fn where_clause(clause: &str) -> Result<()> {
    transform(
        Op::Where {
            clause: clause.to_string(),
        },
        "failed to filter data",
    )
}

pub fn limit(count: &str) -> Result<()> {
    transform(
        Op::Limit {
            count: count.to_string(),
        },
        "failed to limit rows",
    )
}

pub fn offset(count: &str) -> Result<()> {
    transform(
        Op::Offset {
            count: count.to_string(),
        },
        "failed to offset rows",
    )
}

pub fn order_by(clause: &str) -> Result<()> {
    transform(
        Op::OrderBy {
            clause: clause.to_string(),
        },
        "failed to order rows",
    )
}

pub fn describe() -> Result<()> {
    transform(Op::Describe, "failed to describe input")
}

pub fn summarize() -> Result<()> {
    transform(Op::Summarize, "failed to summarize input")
}

fn transform(op: Op, context: &'static str) -> Result<()> {
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
        let execution = open_connection(&plan)
            .and_then(|conn| print_pretty_query(&conn, &plan.compile_sql()))
            .context(context);
        finish_stdin_payload(payload_thread);
        match execution {
            Err(error) if is_broken_pipe(&error) => Ok(()),
            result => result,
        }
    } else {
        write_plan_and_payload(&plan, Some(input), io::stdout().lock()).context(context)
    }
}

fn open_connection(plan: &Plan) -> Result<Connection> {
    let conn = Connection::open_in_memory().context("failed to open duckdb")?;
    storage::prepare(&conn, plan)?;
    Ok(conn)
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
        InputFormat::S3(uri) => Ok(Plan::from_path(uri)),
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
    use std::fs;

    use tempfile::tempdir;

    #[test]
    fn copy_materializes_the_complete_plan_without_a_source_table() {
        let plan = Plan::from_path("/tmp/input.parquet")
            .with_op(Op::Where {
                clause: "age > 40".to_string(),
            })
            .with_op(Op::Select {
                columns: "name".to_string(),
            });

        let query = copy_query(&plan, "'/dev/stdout' (FORMAT CSV)");

        assert_eq!(
            query,
            format!(
                "CREATE TEMP TABLE dq_result AS {}; COPY dq_result TO '/dev/stdout' (FORMAT CSV);",
                plan.compile_sql()
            )
        );
        assert!(!query.contains("dq_source"));
    }

    #[test]
    fn complete_plan_preserves_parquet_partition_and_projection_pushdown() {
        let temp = tempdir().unwrap();
        let first_dir = temp.path().join("date=2026-08-16");
        let second_dir = temp.path().join("date=2026-08-17");
        fs::create_dir_all(&first_dir).unwrap();
        fs::create_dir_all(&second_dir).unwrap();

        let conn = Connection::open_in_memory().unwrap();
        for (path, value) in [
            (first_dir.join("part.parquet"), 1),
            (second_dir.join("part.parquet"), 2),
        ] {
            conn.execute_batch(&format!(
                "COPY (SELECT {value} AS projected, {value} * 10 AS unprojected) TO '{}' (FORMAT PARQUET);",
                path.to_string_lossy().replace('\'', "''")
            ))
            .unwrap();
        }

        let glob = temp.path().join("date=*/part.parquet");
        let plan = Plan::from_path(glob.to_string_lossy())
            .with_op(Op::Where {
                clause: "date = DATE '2026-08-17'".to_string(),
            })
            .with_op(Op::Select {
                columns: "projected".to_string(),
            });
        let mut statement = conn
            .prepare(&format!("EXPLAIN {}", plan.compile_sql()))
            .unwrap();
        let physical_plan = statement
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<duckdb::Result<Vec<_>>>()
            .unwrap()
            .join("\n");

        assert!(physical_plan.contains("File Filters:"), "{physical_plan}");
        assert!(
            physical_plan.contains("Scanning Files: 1/2"),
            "{physical_plan}"
        );
        assert!(physical_plan.contains("Projections:"), "{physical_plan}");
        assert!(physical_plan.contains("projected"), "{physical_plan}");
        assert!(!physical_plan.contains("unprojected"), "{physical_plan}");
    }

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

    #[test]
    fn preserves_s3_uri_in_source_plan() {
        let uri = "s3://bucket/path/data.parquet";

        let plan = build_source_plan(&InputFormat::S3(uri.into())).unwrap();

        assert_eq!(plan, Plan::from_path(uri));
    }
}
