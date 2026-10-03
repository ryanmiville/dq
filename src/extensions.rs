use anyhow::{Context, Result};
use duckdb::Connection;

pub fn load_or_install(conn: &Connection, name: &str, repository: Option<&str>) -> Result<()> {
    if conn.execute_batch(&format!("LOAD {name};")).is_ok() {
        return Ok(());
    }
    let repository = repository
        .map(|repository| format!(" FROM {repository}"))
        .unwrap_or_default();
    conn.execute_batch(&format!("INSTALL {name}{repository}; LOAD {name};"))
        .with_context(|| format!("failed to install or load DuckDB `{name}` extension"))
}
