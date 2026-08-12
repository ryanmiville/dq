use std::path::Path;

#[derive(Debug, Eq, PartialEq)]
pub enum InputFormat {
    Csv,
    Json,
    JsonArray,
    Path(String),
    Passthrough(String),
}

#[derive(Debug, Eq, PartialEq)]
pub enum OutputFormat {
    Csv,
    Json,
    JsonArray,
    Pretty,
    Path(String),
    Passthrough(String),
}

pub enum OutputExecution {
    Copy(String),
    Pretty,
}

impl InputFormat {
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
            _ if file_like(&value) => Self::Path(value),
            _ => Self::Passthrough(value),
        }
    }

    pub fn read_fn(&self) -> String {
        match self {
            Self::Json | Self::JsonArray => "read_json_auto('/dev/stdin')".to_string(),
            Self::Csv => "read_csv('/dev/stdin')".to_string(),
            Self::Path(path) => sql_string_literal(path),
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
            Self::Csv => OutputExecution::Copy(
                "'/dev/stdout' (FORMAT csv, DELIMITER ',', HEADER)".to_string(),
            ),
            Self::Path(path) => OutputExecution::Copy(sql_string_literal(path)),
            Self::Passthrough(text) => OutputExecution::Copy(text.clone()),
        }
    }
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
        assert_eq!(InputFormat::parse_arg("json".into()), InputFormat::Json);
        assert_eq!(InputFormat::parse_arg("csv".into()), InputFormat::Csv);
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
            InputFormat::parse_arg("../testdata.json".into()),
            InputFormat::Path("../testdata.json".into())
        );
        assert_eq!(
            OutputFormat::parse_arg("out.csv".into()),
            OutputFormat::Path("out.csv".into())
        );
    }

    #[test]
    fn preserves_common_passthrough_expressions() {
        assert_eq!(
            InputFormat::parse_arg("read_csv('/dev/stdin')".into()),
            InputFormat::Passthrough("read_csv('/dev/stdin')".into())
        );
        assert_eq!(
            OutputFormat::parse_arg("'/dev/stdout' (FORMAT CSV, HEADER)".into()),
            OutputFormat::Passthrough("'/dev/stdout' (FORMAT CSV, HEADER)".into())
        );
    }
}
