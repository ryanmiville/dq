mod cmd;
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
    /// Read data from stdin in the given format and output Arrow
    ///
    /// Presets: csv, json, jsonl. Any other value is passed directly
    /// as a DuckDB table function expression (e.g. "read_parquet('/dev/stdin')").
    From {
        /// Input format preset or DuckDB read expression
        format: String,
    },

    /// Read Arrow from stdin and write to stdout in the given format
    ///
    /// Presets: csv, json, jsonl, pretty. Any other value is passed directly
    /// as a DuckDB COPY destination expression.
    To {
        /// Output format preset or DuckDB COPY expression
        format: String,
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
        Command::From { format } => cmd::from(&conn, &Format::parse(format)),
        Command::To { format } => cmd::to(&conn, &Format::parse(format)),
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
