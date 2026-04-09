use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

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

    fn validate(&self) -> Result<()> {
        if self.version != PLAN_VERSION {
            anyhow::bail!("unsupported dq plan version: {}", self.version);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Op, Plan};

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
}
