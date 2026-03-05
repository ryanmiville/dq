mod data_type;
mod select;
mod where_clause;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use data_type::{DataType, ToTarget};
use duckdb::Connection;
use select::select;
use where_clause::where_clause;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    From {
        #[arg(value_enum)]
        data_type: DataType,
    },
    To {
        target: String,
    },
    Select {
        columns: String,
    },
    Where {
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
    let connection = open_connection()?;

    match cli.command {
        Command::From { data_type } => data_type.from_stdin(&connection),
        Command::To { target } => ToTarget::parse(target).to_stdout(&connection),
        Command::Select { columns } => select(&connection, &columns),
        Command::Where { clause } => where_clause(&connection, &clause),
    }
}

fn open_connection() -> Result<Connection> {
    let connection = Connection::open_in_memory().context("failed to open duckdb")?;
    connection
        .execute_batch("INSTALL arrow FROM community; LOAD arrow;")
        .context("failed to install and load duckdb arrow extension")?;

    Ok(connection)
}
