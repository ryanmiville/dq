use std::{env, process};

use anyhow::{Context, Result, anyhow};
use duckdb::Connection;

use crate::plan::{Plan, Source};

const CA_CERT_FILE_ENV: &str = "DQ_CA_CERT_FILE";

pub fn prepare(conn: &Connection, plan: &Plan) -> Result<()> {
    let Some(uri) = s3_uri(&plan.source) else {
        return Ok(());
    };

    load_or_install(conn, "httpfs")?;
    configure_ca_cert_file(conn)?;

    if !has_matching_s3_secret(conn, uri)? {
        load_or_install(conn, "aws")?;
        let secret_name = format!("dq_s3_{}", process::id());
        conn.execute_batch(&credential_chain_secret_sql(&secret_name))
            .context("failed to configure S3 access")?;
    }

    Ok(())
}

fn credential_chain_secret_sql(secret_name: &str) -> String {
    // Without eager validation, an empty chain remains anonymous while any
    // credentials found by the chain are still used to sign private requests.
    format!(
        "CREATE TEMPORARY SECRET {secret_name} (\
            TYPE s3, \
            PROVIDER credential_chain, \
            VALIDATION 'none', \
            REFRESH auto\
        );"
    )
}

fn configure_ca_cert_file(conn: &Connection) -> Result<()> {
    let path = match env::var(CA_CERT_FILE_ENV) {
        Ok(path) => path,
        Err(env::VarError::NotPresent) => return Ok(()),
        Err(env::VarError::NotUnicode(_)) => {
            return Err(anyhow!("{CA_CERT_FILE_ENV} path is not valid UTF-8"));
        }
    };

    conn.execute_batch(&ca_cert_file_sql(&path))
        .with_context(|| format!("failed to configure {CA_CERT_FILE_ENV} for S3"))
}

fn ca_cert_file_sql(path: &str) -> String {
    format!("SET ca_cert_file = '{}';", path.replace('\'', "''"))
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
    use super::{ca_cert_file_sql, credential_chain_secret_sql, s3_uri};
    use crate::plan::{Plan, Source};

    #[test]
    fn s3_credentials_are_optional() {
        let sql = credential_chain_secret_sql("dq_test");

        assert!(sql.contains("VALIDATION 'none'"));
        assert!(sql.contains("REFRESH auto"));
    }

    #[test]
    fn escapes_ca_bundle_paths_for_duckdb() {
        assert_eq!(
            ca_cert_file_sql("/tmp/company's-ca.pem"),
            "SET ca_cert_file = '/tmp/company''s-ca.pem';"
        );
    }

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
