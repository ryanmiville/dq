mod cmd;
mod duckbox;
mod format;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use duckdb::Connection;
use format::Format;

/// Shell-first data pipelines powered by DuckDB.
///
/// Pipe-compose subcommands to build data pipelines. Stages exchange Arrow
/// format over stdin/stdout, so you can chain them with Unix pipes.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read data from stdin or a file and output Arrow
    ///
    /// Presets: csv, json, jsonl. Any other path is treated as a file source.
    /// Use --expr for raw DuckDB read expressions.
    From {
        /// Input format preset or input file path
        #[arg(value_name = "FORMAT|PATH", required_unless_present = "expr")]
        format: Option<String>,

        /// Raw DuckDB read expression (escape hatch for advanced use)
        #[arg(long, value_name = "READ_EXPR", conflicts_with = "format")]
        expr: Option<String>,
    },

    /// Read Arrow from stdin and write to stdout in the given format
    ///
    /// Presets: csv, json, jsonl, pretty. Any other path is treated as a file
    /// destination. Use --expr for raw DuckDB COPY expressions.
    To {
        /// Output format preset or output file path
        #[arg(value_name = "FORMAT|PATH", required_unless_present = "expr")]
        format: Option<String>,

        /// Raw DuckDB COPY expression (escape hatch for advanced use)
        #[arg(long, value_name = "COPY_EXPR", conflicts_with = "format")]
        expr: Option<String>,
    },

    /// Project columns from Arrow input using a SQL SELECT expression
    ///
    /// The expression is interpolated into SELECT <columns> FROM stdin.
    Select {
        /// SQL column expression (e.g. "name, age * 2 AS double_age")
        columns: String,
    },

    /// Filter rows from Arrow input using a SQL WHERE expression
    ///
    /// The expression is interpolated into SELECT * FROM stdin WHERE <clause>.
    Where {
        /// SQL boolean expression (e.g. "age > 30 AND name LIKE 'A%'")
        clause: String,
    },
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
    }
}

fn open_connection() -> Result<Connection> {
    let conn = Connection::open_in_memory().context("failed to open duckdb")?;
    conn.execute_batch("INSTALL arrow FROM community; LOAD arrow;")
        .context("failed to install and load duckdb arrow extension")?;
    Ok(conn)
}
