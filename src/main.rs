mod data_type;

use anyhow::{Context, Ok, Result};
use clap::{Parser, Subcommand};
use data_type::DataType;
use duckdb::Connection;

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
        #[arg(value_enum)]
        data_type: DataType,
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
        Command::To { data_type } => data_type.to_stdout(&connection),
    }
}

fn open_connection() -> Result<Connection> {
    let connection = Connection::open_in_memory().context("failed to open duckdb")?;
    connection
        .execute_batch("INSTALL arrow FROM community; LOAD arrow;")
        .context("failed to install and load duckdb arrow extension")?;

    Ok(connection)
}
