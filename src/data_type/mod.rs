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

pub enum ToTarget {
    Preset(DataType),
    RawOptions(String),
}

impl ToTarget {
    pub fn parse(value: String) -> Self {
        match DataType::from_str(&value, true) {
            Ok(data_type) => ToTarget::Preset(data_type),
            Err(_) => ToTarget::RawOptions(value),
        }
    }

    pub fn to_stdout(&self, connection: &Connection) -> Result<()> {
        match self {
            ToTarget::Preset(data_type) => data_type.to_stdout(connection),
            ToTarget::RawOptions(copy_options) => {
                let sql = format!(
                    "CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin'); COPY dq_input TO '/dev/stdout' {copy_options};"
                );
                connection
                    .execute_batch(&sql)
                    .context("failed to convert stdin to output format")
            }
        }
    }
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
