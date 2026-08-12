use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const PLAN_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub version: u32,
    pub source: Source,
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Path { path: String },
    Stream { read_expr: String },
}

impl Source {
    pub fn is_stream(&self) -> bool {
        matches!(self, Self::Stream { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Op {
    Select { columns: String },
    Where { clause: String },
    OrderBy { clause: String },
    Limit { count: String },
    Offset { count: String },
    Describe,
    Summarize,
}

impl Plan {
    pub fn from_path(path: impl Into<String>) -> Self {
        Self::new(Source::Path { path: path.into() })
    }

    pub fn from_stream(read_expr: impl Into<String>) -> Self {
        Self::new(Source::Stream {
            read_expr: read_expr.into(),
        })
    }

    fn new(source: Source) -> Self {
        Self {
            version: PLAN_VERSION,
            source,
            ops: Vec::new(),
        }
    }

    pub fn with_op(mut self, op: Op) -> Self {
        self.ops.push(op);
        self
    }

    pub fn from_json_slice(json: &[u8]) -> Result<Self> {
        let plan = serde_json::from_slice::<Self>(json).context("failed to parse dq plan json")?;
        plan.validate()?;
        Ok(plan)
    }

    pub fn to_json_vec(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).context("failed to serialize dq plan json")
    }

    pub fn compile_sql(&self) -> String {
        self.compile_from(base_query(&self.source))
    }

    pub fn compile_sql_from_table(&self, table: &str) -> String {
        self.compile_from(format!("SELECT * FROM {table}"))
    }

    fn compile_from(&self, query: String) -> String {
        self.ops.iter().fold(query, compile_op)
    }

    fn validate(&self) -> Result<()> {
        if self.version != PLAN_VERSION {
            anyhow::bail!("unsupported dq plan version: {}", self.version);
        }
        Ok(())
    }
}

fn base_query(source: &Source) -> String {
    match source {
        Source::Path { path } => format!("SELECT * FROM {}", sql_string_literal(path)),
        Source::Stream { read_expr } => format!("SELECT * FROM {read_expr}"),
    }
}

fn compile_op(input: String, op: &Op) -> String {
    match op {
        Op::Select { columns } => format!("SELECT {columns} FROM ({input}) AS q"),
        Op::Where { clause } => format!("SELECT * FROM ({input}) AS q WHERE {clause}"),
        Op::OrderBy { clause } => format!("SELECT * FROM ({input}) AS q ORDER BY {clause}"),
        Op::Limit { count } => format!("SELECT * FROM ({input}) AS q LIMIT {count}"),
        Op::Offset { count } => format!("SELECT * FROM ({input}) AS q OFFSET {count}"),
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

    #[test]
    fn round_trips_stream_plan_json() {
        let plan = Plan::from_stream("read_json_auto('/dev/stdin')").with_op(Op::Where {
            clause: "age > 40".to_string(),
        });

        let json = plan.to_json_vec().unwrap();
        let decoded = Plan::from_json_slice(&json).unwrap();

        assert_eq!(decoded, plan);
        let text = String::from_utf8(json).unwrap();
        assert!(text.contains("\"version\":1"));
        assert!(text.contains("\"kind\":\"stream\""));
        assert!(text.contains("\"read_expr\":\"read_json_auto('/dev/stdin')\""));
        assert!(text.contains("\"kind\":\"where\""));
    }

    #[test]
    fn round_trips_path_plan_json() {
        let plan = Plan::from_path("/tmp/input.parquet");

        assert_eq!(
            Plan::from_json_slice(&plan.to_json_vec().unwrap()).unwrap(),
            plan
        );
    }

    #[test]
    fn rejects_unsupported_plan_version() {
        let error = Plan::from_json_slice(
            br#"{"version":2,"source":{"kind":"path","path":"input.json"},"ops":[]}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("unsupported dq plan version: 2"));
    }

    #[test]
    fn preserves_op_order_in_json_round_trip() {
        let plan = Plan::from_path("input.json")
            .with_op(Op::Where {
                clause: "age > 40".to_string(),
            })
            .with_op(Op::OrderBy {
                clause: "age desc".to_string(),
            })
            .with_op(Op::Select {
                columns: "name".to_string(),
            });

        let decoded = Plan::from_json_slice(&plan.to_json_vec().unwrap()).unwrap();

        assert_eq!(decoded.ops, plan.ops);
    }

    #[test]
    fn compiles_stream_source_and_where_to_nested_sql() {
        let sql = Plan::from_stream("read_json_auto('/dev/stdin')")
            .with_op(Op::Where {
                clause: "age > 40".to_string(),
            })
            .compile_sql();

        assert_eq!(
            sql,
            "SELECT * FROM (SELECT * FROM read_json_auto('/dev/stdin')) AS q WHERE age > 40"
        );
    }

    #[test]
    fn preserves_transform_order_when_compiling_sql() {
        let sql = Plan::from_path("/tmp/input.parquet")
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
        let sql = Plan::from_path("/tmp/ada's.parquet")
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
        let sql = Plan::from_path("/tmp/input.parquet")
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
}
