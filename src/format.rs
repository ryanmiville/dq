use clap::ValueEnum;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Preset {
    Csv,
    Json,
    Jsonl,
}

pub enum Format {
    Preset(Preset),
    Passthrough(String),
}

impl Format {
    pub fn parse(value: String) -> Self {
        match Preset::from_str(&value, true) {
            Ok(format) => Format::Preset(format),
            Err(_) => Format::Passthrough(value),
        }
    }

    pub fn read_fn(&self) -> &str {
        match self {
            Format::Preset(Preset::Json) => "read_json_auto('/dev/stdin')",
            Format::Preset(Preset::Jsonl) => {
                "read_json_auto('/dev/stdin', format='newline_delimited')"
            }
            Format::Preset(Preset::Csv) => "read_csv('/dev/stdin')",
            Format::Passthrough(text) => text,
        }
    }

    pub fn copy_format(&self) -> &str {
        match self {
            Format::Preset(Preset::Json) => "(FORMAT JSON, ARRAY true)",
            Format::Preset(Preset::Jsonl) => "(FORMAT JSON, ARRAY false)",
            Format::Preset(Preset::Csv) => "(FORMAT csv, DELIMITER ',', HEADER)",
            Format::Passthrough(text) => text,
        }
    }
}
