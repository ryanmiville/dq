use std::path::Path;

use anyhow::{Result, bail};

#[derive(Debug, Eq, PartialEq)]
pub enum InputFormat {
    Csv,
    Json,
    JsonArray,
    Path(String),
    S3(String),
    Passthrough(String),
}

#[derive(Debug, Eq, PartialEq)]
pub enum OutputFormat {
    Csv,
    Json,
    JsonArray,
    Yaml,
    Pretty,
    Path(String),
    Passthrough(String),
}

pub enum OutputExecution {
    Copy(String),
    Pretty,
}

impl InputFormat {
    pub fn parse(value: Option<String>, expr: Option<String>) -> Result<Self> {
        match (value, expr) {
            (_, Some(expr)) => Ok(Self::Passthrough(expr)),
            (Some(value), None) => Self::parse_arg(value),
            (None, None) => unreachable!("clap guarantees either a positional value or --expr"),
        }
    }

    fn parse_arg(value: String) -> Result<Self> {
        Ok(match value.to_ascii_lowercase().as_str() {
            "csv" => Self::Csv,
            "json" => Self::Json,
            "json-array" => Self::JsonArray,
            "yaml" => bail!(
                "YAML stdin is not supported by the DuckDB YAML extension; use `dq from <file.yaml>` or `dq from <file.yml>`"
            ),
            _ if is_s3_uri(&value) => Self::S3(value),
            _ if file_like(&value) => Self::Path(value),
            _ => Self::Passthrough(value),
        })
    }

    pub fn read_fn(&self) -> String {
        match self {
            Self::Json | Self::JsonArray => "read_json_auto('/dev/stdin')".to_string(),
            Self::Csv => "read_csv('/dev/stdin')".to_string(),
            Self::Path(path) | Self::S3(path) => sql_string_literal(path),
            Self::Passthrough(text) => text.clone(),
        }
    }
}

impl OutputFormat {
    pub fn parse(value: Option<String>, expr: Option<String>) -> Self {
        match (value, expr) {
            (_, Some(expr)) => Self::Passthrough(expr),
            (Some(value), None) => Self::parse_arg(value),
            (None, None) => unreachable!("clap guarantees either a positional value or --expr"),
        }
    }

    fn parse_arg(value: String) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "csv" => Self::Csv,
            "json" => Self::Json,
            "json-array" => Self::JsonArray,
            "yaml" => Self::Yaml,
            "pretty" => Self::Pretty,
            _ if file_like(&value) => Self::Path(value),
            _ => Self::Passthrough(value),
        }
    }

    pub fn execution(&self) -> OutputExecution {
        match self {
            Self::Pretty => OutputExecution::Pretty,
            Self::Json => {
                OutputExecution::Copy("'/dev/stdout' (FORMAT JSON, ARRAY false)".to_string())
            }
            Self::JsonArray => {
                OutputExecution::Copy("'/dev/stdout' (FORMAT JSON, ARRAY true)".to_string())
            }
            Self::Yaml => OutputExecution::Copy("'/dev/stdout' (FORMAT YAML)".to_string()),
            Self::Csv => OutputExecution::Copy(
                "'/dev/stdout' (FORMAT csv, DELIMITER ',', HEADER)".to_string(),
            ),
            Self::Path(path) if is_yaml_path(path) => {
                OutputExecution::Copy(format!("{} (FORMAT YAML)", sql_string_literal(path)))
            }
            Self::Path(path) => OutputExecution::Copy(sql_string_literal(path)),
            Self::Passthrough(text) => OutputExecution::Copy(text.clone()),
        }
    }

    pub fn requires_yaml(&self) -> bool {
        match self {
            Self::Yaml => true,
            Self::Path(path) => is_yaml_path(path),
            _ => false,
        }
    }
}

pub fn is_yaml_path(value: &str) -> bool {
    Path::new(value)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("yaml") || extension.eq_ignore_ascii_case("yml")
        })
}

fn is_s3_uri(value: &str) -> bool {
    value.starts_with("s3://")
}

fn file_like(value: &str) -> bool {
    Path::new(value).extension().is_some()
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::{InputFormat, OutputExecution, OutputFormat};

    #[test]
    fn parses_input_presets_before_paths() {
        assert_eq!(
            InputFormat::parse_arg("json".into()).unwrap(),
            InputFormat::Json
        );
        assert_eq!(
            InputFormat::parse_arg("csv".into()).unwrap(),
            InputFormat::Csv
        );
    }

    #[test]
    fn parses_output_pretty_preset() {
        assert_eq!(
            OutputFormat::parse_arg("pretty".into()),
            OutputFormat::Pretty
        );
        assert!(matches!(
            OutputFormat::Pretty.execution(),
            OutputExecution::Pretty
        ));
    }

    #[test]
    fn parses_paths_without_sql_quotes() {
        assert_eq!(
            InputFormat::parse_arg("../testdata.json".into()).unwrap(),
            InputFormat::Path("../testdata.json".into())
        );
        assert_eq!(
            OutputFormat::parse_arg("out.csv".into()),
            OutputFormat::Path("out.csv".into())
        );
    }

    #[test]
    fn parses_s3_uris_with_or_without_file_extensions() {
        assert_eq!(
            InputFormat::parse_arg("s3://bucket/data.parquet".into()).unwrap(),
            InputFormat::S3("s3://bucket/data.parquet".into())
        );
        assert_eq!(
            InputFormat::parse_arg("s3://bucket/dataset".into()).unwrap(),
            InputFormat::S3("s3://bucket/dataset".into())
        );
    }

    #[test]
    fn preserves_common_passthrough_expressions() {
        assert_eq!(
            InputFormat::parse_arg("read_csv('/dev/stdin')".into()).unwrap(),
            InputFormat::Passthrough("read_csv('/dev/stdin')".into())
        );
        assert_eq!(
            OutputFormat::parse_arg("'/dev/stdout' (FORMAT CSV, HEADER)".into()),
            OutputFormat::Passthrough("'/dev/stdout' (FORMAT CSV, HEADER)".into())
        );
    }
}
