use std::process;

use anyhow::{Context, Result};
use duckdb::Connection;

use crate::plan::{Plan, Source};

pub fn prepare(conn: &Connection, plan: &Plan) -> Result<()> {
    let Some(uri) = s3_uri(&plan.source) else {
        return Ok(());
    };

    load_or_install(conn, "httpfs")?;

    if !has_matching_s3_secret(conn, uri)? {
        load_or_install(conn, "aws")?;
        let secret_name = format!("dq_s3_{}", process::id());
        conn.execute_batch(&format!(
            "CREATE TEMPORARY SECRET {secret_name} (\
                TYPE s3, \
                PROVIDER credential_chain, \
                VALIDATION 'exists'\
            );"
        ))
        .context("failed to load AWS credentials for S3")?;
    }

    Ok(())
}

fn s3_uri(source: &Source) -> Option<&str> {
    match source {
        Source::Path { path } if is_s3_uri(path) => Some(path),
        _ => None,
    }
}

fn is_s3_uri(value: &str) -> bool {
    value.starts_with("s3://")
}

fn load_or_install(conn: &Connection, extension: &str) -> Result<()> {
    if conn.execute_batch(&format!("LOAD {extension};")).is_ok() {
        return Ok(());
    }

    conn.execute_batch(&format!("INSTALL {extension}; LOAD {extension};"))
        .with_context(|| format!("failed to install or load DuckDB `{extension}` extension"))
}

fn has_matching_s3_secret(conn: &Connection, uri: &str) -> Result<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM which_secret(?1, 's3');",
            [uri],
            |row| row.get(0),
        )
        .context("failed to inspect DuckDB S3 credentials")?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::s3_uri;
    use crate::plan::{Plan, Source};

    #[test]
    fn identifies_s3_plan_sources() {
        let plan = Plan::from_path("s3://bucket/path/data.parquet");

        assert_eq!(s3_uri(&plan.source), Some("s3://bucket/path/data.parquet"));
    }

    #[test]
    fn ignores_local_and_stream_sources() {
        assert_eq!(
            s3_uri(&Source::Path {
                path: "/tmp/data.parquet".into()
            }),
            None
        );
        assert_eq!(
            s3_uri(&Source::Stream {
                read_expr: "read_json_auto('/dev/stdin')".into(),
            }),
            None
        );
    }
}
