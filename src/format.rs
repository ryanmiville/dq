use std::path::Path;

use clap::ValueEnum;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Preset {
    Csv,
    Json,
    JsonArray,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Format {
    Preset(Preset),
    Path(String),
    Passthrough(String),
}

impl Format {
    pub fn parse(value: Option<String>, expr: Option<String>) -> Self {
        match (value, expr) {
            (_, Some(expr)) => Format::Passthrough(expr),
            (Some(value), None) => Self::parse_arg(value),
            (None, None) => unreachable!("clap guarantees either a positional value or --expr"),
        }
    }

    fn parse_arg(value: String) -> Self {
        match Preset::from_str(&value, true) {
            Ok(format) => Format::Preset(format),
            Err(_) if file_like(&value) => Format::Path(value),
            Err(_) => Format::Passthrough(value),
        }
    }

    pub fn read_fn(&self) -> String {
        match self {
            Format::Preset(Preset::Json) => "read_json_auto('/dev/stdin')".to_string(),
            Format::Preset(Preset::JsonArray) => "read_json_auto('/dev/stdin')".to_string(),
            Format::Preset(Preset::Csv) => "read_csv('/dev/stdin')".to_string(),
            Format::Path(path) => sql_string_literal(path),
            Format::Passthrough(text) => text.clone(),
        }
    }

    pub fn copy_format(&self) -> String {
        match self {
            Format::Preset(Preset::Json) => "'/dev/stdout' (FORMAT JSON, ARRAY false)".to_string(),
            Format::Preset(Preset::JsonArray) => {
                "'/dev/stdout' (FORMAT JSON, ARRAY true)".to_string()
            }
            Format::Preset(Preset::Csv) => {
                "'/dev/stdout' (FORMAT csv, DELIMITER ',', HEADER)".to_string()
            }
            Format::Path(path) => sql_string_literal(path),
            Format::Passthrough(text) => text.clone(),
        }
    }
}

fn file_like(value: &str) -> bool {
    Path::new(value).extension().is_some()
}

// fn looks_like_duckdb_expr(value: &str) -> bool {
//     let value = value.trim();
//     value.starts_with('\'')
//         || value.starts_with('"')
//         || value.starts_with('(')
//         || is_function_call(value)
// }

// fn is_function_call(value: &str) -> bool {
//     let value = value.trim();
//     if !value.ends_with(')') {
//         return false;
//     }

//     let Some(open_paren) = value.find('(') else {
//         return false;
//     };

//     if open_paren == 0 {
//         return false;
//     }

//     value[..open_paren]
//         .chars()
//         .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
// }

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::{Format, Preset};

    #[test]
    fn parses_presets_before_paths() {
        assert_eq!(
            Format::parse_arg("json".into()),
            Format::Preset(Preset::Json)
        );
        assert_eq!(Format::parse_arg("csv".into()), Format::Preset(Preset::Csv));
    }

    #[test]
    fn parses_paths_without_sql_quotes() {
        assert_eq!(
            Format::parse_arg("../testdata.json".into()),
            Format::Path("../testdata.json".into())
        );
        assert_eq!(
            Format::parse_arg("out.csv".into()),
            Format::Path("out.csv".into())
        );
    }

    #[test]
    fn preserves_common_passthrough_expressions() {
        assert_eq!(
            Format::parse_arg("read_csv('/dev/stdin')".into()),
            Format::Passthrough("read_csv('/dev/stdin')".into())
        );
        assert_eq!(
            Format::parse_arg("'/dev/stdout' (FORMAT CSV, HEADER)".into()),
            Format::Passthrough("'/dev/stdout' (FORMAT CSV, HEADER)".into())
        );
    }
}
