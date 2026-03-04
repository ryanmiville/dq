mod csv;
mod json;
mod jsonl;

use anyhow::{Context, Result};
use clap::ValueEnum;
use duckdb::Connection;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum DataType {
    Csv,
    Json,
    Jsonl,
}
impl DataType {
    pub fn from_stdin(self, connection: &Connection) -> Result<()> {
        let sql = match self {
            DataType::Csv => csv::FROM_SQL,
            DataType::Json => json::FROM_SQL,
            DataType::Jsonl => jsonl::FROM_SQL,
        };
        connection
            .execute_batch(sql)
            .context("failed to convert stdin to arrow")
    }

    pub fn to_stdout(self, connection: &Connection) -> Result<()> {
        let sql = match self {
            DataType::Csv => csv::TO_SQL,
            DataType::Json => json::TO_SQL,
            DataType::Jsonl => jsonl::TO_SQL,
        };
        connection
            .execute_batch(sql)
            .context("failed to convert stdin to output format")
    }
}
