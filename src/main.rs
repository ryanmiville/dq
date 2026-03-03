use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
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

#[derive(Clone, Copy, ValueEnum)]
enum DataType {
    Jsonl,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let connection = Connection::open_in_memory().context("failed to open duckdb")?;

    load_arrow_extension(&connection)?;

    match cli.command {
        Command::From {
            data_type: DataType::Jsonl,
        } => from_jsonl(&connection),
        Command::To {
            data_type: DataType::Jsonl,
        } => to_jsonl(&connection),
    }
}

fn load_arrow_extension(connection: &Connection) -> Result<()> {
    connection
        .execute_batch("INSTALL arrow FROM community; LOAD arrow;")
        .context("failed to install and load duckdb arrow extension")
}

fn from_jsonl(connection: &Connection) -> Result<()> {
    connection
        .execute_batch(
            "COPY (SELECT * FROM read_json_auto('/dev/stdin', format='newline_delimited')) TO '/dev/stdout' (FORMAT ARROW);",
        )
        .context("failed to convert jsonl stdin to arrow stdout")
}

fn to_jsonl(connection: &Connection) -> Result<()> {
    connection
        .execute_batch(
            "CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin'); COPY dq_input TO '/dev/stdout' (FORMAT JSON, ARRAY false);",
        )
        .context("failed to convert arrow stdin to jsonl stdout")
}
