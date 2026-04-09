use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

const PLAN_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub version: u32,
    pub source_path: String,
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Op {
    Select { columns: String },
    Where { clause: String },
    OrderBy { clause: String },
    Limit { count: String },
    Describe,
    Summarize,
}

impl Plan {
    pub fn new(source_path: impl Into<String>) -> Self {
        Self {
            version: PLAN_VERSION,
            source_path: source_path.into(),
            ops: Vec::new(),
        }
    }

    pub fn with_op(mut self, op: Op) -> Self {
        self.ops.push(op);
        self
    }

    pub fn from_json_str(json: &str) -> Result<Self> {
        let plan = serde_json::from_str::<Self>(json).context("failed to parse dq plan json")?;
        plan.validate()?;
        Ok(plan)
    }

    pub fn to_json_string(&self) -> Result<String> {
        serde_json::to_string(self).context("failed to serialize dq plan json")
    }

    pub fn compile_sql(&self) -> String {
        self.ops.iter().fold(base_query(&self.source_path), |query, op| {
            compile_op(query, op)
        })
    }

    pub fn write_to(&self, mut writer: impl Write) -> Result<()> {
        writeln!(writer, "{}", self.to_json_string()?).context("failed to write dq plan json")
    }

    pub fn read_from(mut reader: impl Read) -> Result<Self> {
        let mut json = String::new();
        reader
            .read_to_string(&mut json)
            .context("failed to read dq plan json")?;
        Self::from_json_str(&json)
    }

    fn validate(&self) -> Result<()> {
        if self.version != PLAN_VERSION {
            anyhow::bail!("unsupported dq plan version: {}", self.version);
        }
        Ok(())
    }
}

fn base_query(source_path: &str) -> String {
    format!("SELECT * FROM {}", sql_string_literal(source_path))
}

fn compile_op(input: String, op: &Op) -> String {
    match op {
        Op::Select { columns } => format!("SELECT {columns} FROM ({input}) AS q"),
        Op::Where { clause } => format!("SELECT * FROM ({input}) AS q WHERE {clause}"),
        Op::OrderBy { clause } => format!("SELECT * FROM ({input}) AS q ORDER BY {clause}"),
        Op::Limit { count } => format!("SELECT * FROM ({input}) AS q LIMIT {count}"),
        Op::Describe => format!("SELECT * FROM (DESCRIBE SELECT * FROM ({input}) AS q) AS q"),
        Op::Summarize => format!("SELECT * FROM (SUMMARIZE SELECT * FROM ({input}) AS q) AS q"),
    }
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::{Op, Plan};
    use std::io::Cursor;

    #[test]
    fn round_trips_plan_json() {
        let plan = Plan::new("/tmp/input.parquet").with_op(Op::Where {
            clause: "age > 40".to_string(),
        });

        let json = plan.to_json_string().unwrap();
        let decoded = Plan::from_json_str(&json).unwrap();

        assert_eq!(decoded, plan);
        assert!(json.contains("\"version\":1"));
        assert!(json.contains("\"source_path\":\"/tmp/input.parquet\""));
        assert!(json.contains("\"kind\":\"where\""));
    }

    #[test]
    fn rejects_unsupported_plan_version() {
        let err = Plan::from_json_str(
            r#"{"version":2,"source_path":"/tmp/input.parquet","ops":[]}"#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("unsupported dq plan version: 2"));
    }

    #[test]
    fn preserves_op_order_in_json_round_trip() {
        let plan = Plan::new("/tmp/input.parquet")
            .with_op(Op::Where {
                clause: "age > 40".to_string(),
            })
            .with_op(Op::OrderBy {
                clause: "age desc".to_string(),
            })
            .with_op(Op::Select {
                columns: "name".to_string(),
            });

        let decoded = Plan::from_json_str(&plan.to_json_string().unwrap()).unwrap();

        assert_eq!(
            decoded.ops,
            vec![
                Op::Where {
                    clause: "age > 40".to_string(),
                },
                Op::OrderBy {
                    clause: "age desc".to_string(),
                },
                Op::Select {
                    columns: "name".to_string(),
                },
            ]
        );
    }

    #[test]
    fn compiles_where_plan_to_nested_sql() {
        let sql = Plan::new("/tmp/input.parquet")
            .with_op(Op::Where {
                clause: "age > 40".to_string(),
            })
            .compile_sql();

        assert_eq!(
            sql,
            "SELECT * FROM (SELECT * FROM '/tmp/input.parquet') AS q WHERE age > 40"
        );
    }

    #[test]
    fn preserves_transform_order_when_compiling_sql() {
        let sql = Plan::new("/tmp/input.parquet")
            .with_op(Op::Select {
                columns: "name".to_string(),
            })
            .with_op(Op::Where {
                clause: "age > 40".to_string(),
            })
            .compile_sql();

        assert_eq!(
            sql,
            "SELECT * FROM (SELECT name FROM (SELECT * FROM '/tmp/input.parquet') AS q) AS q WHERE age > 40"
        );
    }

    #[test]
    fn compiles_describe_over_prior_pipeline_and_escapes_source_path() {
        let sql = Plan::new("/tmp/ada's.parquet")
            .with_op(Op::Where {
                clause: "age > 40".to_string(),
            })
            .with_op(Op::Describe)
            .compile_sql();

        assert_eq!(
            sql,
            "SELECT * FROM (DESCRIBE SELECT * FROM (SELECT * FROM (SELECT * FROM '/tmp/ada''s.parquet') AS q WHERE age > 40) AS q) AS q"
        );
    }

    #[test]
    fn compiles_summarize_to_relation_sql() {
        let sql = Plan::new("/tmp/input.parquet")
            .with_op(Op::Limit {
                count: "2".to_string(),
            })
            .with_op(Op::Summarize)
            .compile_sql();

        assert_eq!(
            sql,
            "SELECT * FROM (SUMMARIZE SELECT * FROM (SELECT * FROM (SELECT * FROM '/tmp/input.parquet') AS q LIMIT 2) AS q) AS q"
        );
    }

    #[test]
    fn writes_plan_json_with_trailing_newline() {
        let plan = Plan::new("/tmp/input.parquet").with_op(Op::Where {
            clause: "age > 40".to_string(),
        });
        let mut out = Vec::new();

        plan.write_to(&mut out).unwrap();

        let written = String::from_utf8(out).unwrap();
        assert_eq!(written, format!("{}\n", plan.to_json_string().unwrap()));
    }

    #[test]
    fn reads_plan_from_reader() {
        let plan = Plan::new("/tmp/input.parquet").with_op(Op::Where {
            clause: "age > 40".to_string(),
        });
        let mut out = Vec::new();
        plan.write_to(&mut out).unwrap();

        let decoded = Plan::read_from(Cursor::new(out)).unwrap();

        assert_eq!(decoded, plan);
    }

    #[test]
    fn reads_plan_with_trailing_whitespace() {
        let decoded = Plan::read_from(Cursor::new(
            b"{\"version\":1,\"source_path\":\"/tmp/input.parquet\",\"ops\":[]}\n\t  ",
        ))
        .unwrap();

        assert_eq!(decoded, Plan::new("/tmp/input.parquet"));
    }

    #[test]
    fn read_from_reports_invalid_json() {
        let err = Plan::read_from(Cursor::new(b"not json".as_slice())).unwrap_err();

        assert!(err.to_string().contains("failed to parse dq plan json"));
    }

    #[test]
    fn read_from_rejects_unsupported_version() {
        let err = Plan::read_from(Cursor::new(
            b"{\"version\":2,\"source_path\":\"/tmp/input.parquet\",\"ops\":[]}".as_slice(),
        ))
        .unwrap_err();

        assert!(err.to_string().contains("unsupported dq plan version: 2"));
    }
}
