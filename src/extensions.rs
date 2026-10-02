use anyhow::{Context, Result};
use duckdb::Connection;

pub enum Extension {
    Httpfs,
    Aws,
    Yaml,
}

impl Extension {
    fn name(&self) -> &'static str {
        match self {
            Self::Httpfs => "httpfs",
            Self::Aws => "aws",
            Self::Yaml => "yaml",
        }
    }

    fn install_sql(&self) -> String {
        match self {
            Self::Httpfs | Self::Aws => format!("INSTALL {};", self.name()),
            Self::Yaml => "INSTALL yaml FROM community;".to_string(),
        }
    }
}

pub fn load_or_install(conn: &Connection, extension: Extension) -> Result<()> {
    let name = extension.name();
    if conn.execute_batch(&format!("LOAD {name};")).is_ok() {
        return Ok(());
    }

    conn.execute_batch(&format!("{} LOAD {name};", extension.install_sql()))
        .with_context(|| format!("failed to install or load DuckDB `{name}` extension"))
}
