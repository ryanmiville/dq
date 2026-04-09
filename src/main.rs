mod cmd;
mod duckbox;
mod format;
mod plan;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use duckdb::Connection;
use format::Format;

/// Shell-first data pipelines powered by DuckDB.
///
/// Pipe-compose subcommands to build data pipelines. Intermediate stages
/// exchange JSON query plans over stdin/stdout, and terminal writes pretty
/// tables when stdout is a TTY.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read data from stdin or a file and output a query plan
    ///
    /// Presets: csv, json, json-array. Any other path is treated as a file source.
    /// Use --expr for raw DuckDB read expressions.
    From {
        /// Input format preset or input file path
        #[arg(value_name = "FORMAT|PATH", required_unless_present = "expr")]
        format: Option<String>,

        /// Raw DuckDB read expression (escape hatch for advanced use)
        #[arg(long, value_name = "READ_EXPR", conflicts_with = "format")]
        expr: Option<String>,
    },

    /// Read a query plan from stdin and write query results in the given format
    ///
    /// Presets: csv, json, json-array. Any other path is treated as a file
    /// destination. Use --expr for raw DuckDB COPY expressions.
    To {
        /// Output format preset or output file path
        #[arg(value_name = "FORMAT|PATH", required_unless_present = "expr")]
        format: Option<String>,

        /// Raw DuckDB COPY expression (escape hatch for advanced use)
        #[arg(long, value_name = "COPY_EXPR", conflicts_with = "format")]
        expr: Option<String>,
    },

    /// Project columns from planned input using a SQL SELECT expression
    ///
    /// The expression is appended to the query plan as SELECT <columns>.
    Select {
        /// SQL column expression (e.g. "name, age * 2 AS double_age")
        columns: String,
    },

    /// Filter rows from planned input using a SQL WHERE expression
    ///
    /// The expression is appended to the query plan as WHERE <clause>.
    Where {
        /// SQL boolean expression (e.g. "age > 30 AND name LIKE 'A%'")
        clause: String,
    },

    /// Limit the number of rows from planned input
    ///
    /// The count is appended to the query plan as LIMIT <count>.
    Limit {
        /// Maximum number of rows to return
        count: String,
    },

    /// Order rows from planned input using a SQL ORDER BY expression
    ///
    /// The expression is appended to the query plan as ORDER BY <clause>.
    OrderBy {
        /// SQL ORDER BY expression (e.g. "age DESC, name ASC")
        clause: String,
    },

    /// Describe the schema of planned input
    ///
    /// Internally appends a DESCRIBE step to the query plan.
    Describe,

    /// Summarize planned input with column statistics
    ///
    /// Internally appends a SUMMARIZE step to the query plan.
    Summarize,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let conn = open_connection()?;

    match cli.command {
        Command::From { format, expr } => cmd::from(&conn, &Format::parse(format, expr)),
        Command::To { format, expr } => cmd::to(&conn, &Format::parse(format, expr)),
        Command::Select { columns } => cmd::select(&conn, &columns),
        Command::Where { clause } => cmd::where_clause(&conn, &clause),
        Command::Limit { count } => cmd::limit(&conn, &count),
        Command::OrderBy { clause } => cmd::order_by(&conn, &clause),
        Command::Describe => cmd::describe(&conn),
        Command::Summarize => cmd::summarize(&conn),
    }
}

fn open_connection() -> Result<Connection> {
    Connection::open_in_memory().context("failed to open duckdb")
}
