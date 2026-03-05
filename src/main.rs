mod cmd;
mod format;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use duckdb::Connection;
use format::Format;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    From { format: String },
    To { format: String },
    Select { columns: String },
    Where { clause: String },
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
