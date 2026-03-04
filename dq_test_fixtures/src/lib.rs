use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Suite {
    pub cmd: String,
    pub cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub name: String,
    pub input: String,
    #[serde(default = "default_success")]
    pub success: bool,
    pub expect: Expect,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum Expect {
    Same,
    Exact { stdout: String },
    StderrContains { text: String },
}

const fn default_success() -> bool {
    true
}

pub fn load_suite_from_path(path: &Path) -> Result<Suite, String> {
    let content = fs::read_to_string(path)
        .map_err(|err| format!("fixture `{}` read error: {err}", path.display()))?;
    parse_and_validate_suite(&content, &path.display().to_string())
}

pub fn parse_and_validate_suite(content: &str, fixture_label: &str) -> Result<Suite, String> {
    let suite: Suite = toml::from_str(content)
        .map_err(|err| format!("fixture `{fixture_label}` parse error: {err}"))?;
    validate_suite(&suite, fixture_label)?;
    Ok(suite)
}

pub fn validate_suite(suite: &Suite, fixture_label: &str) -> Result<(), String> {
    if suite.cmd.trim().is_empty() {
        return Err(format!(
            "fixture `{fixture_label}` invalid: cmd must be non-empty"
        ));
    }

    if suite.cases.is_empty() {
        return Err(format!(
            "fixture `{fixture_label}` invalid: must declare at least one case"
        ));
    }

    let mut seen_names = HashSet::new();

    for case in &suite.cases {
        if case.name.trim().is_empty() {
            return Err(format!(
                "fixture `{fixture_label}` invalid: case name must be non-empty"
            ));
        }

        if !seen_names.insert(case.name.clone()) {
            return Err(format!(
                "fixture `{fixture_label}` invalid: duplicate case name `{}`",
                case.name
            ));
        }

        if !case.success && matches!(case.expect, Expect::Same) {
            return Err(format!(
                "fixture `{fixture_label}` case `{}` invalid: success=false cannot use expect.kind=same",
                case.name
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_and_validate_suite;

    #[test]
    fn rejects_empty_cmd() {
        let err = parse_and_validate_suite("cmd = \"\"\n\n[[cases]]\nname = \"ok\"\ninput = \"a\"\n[cases.expect]\nkind = \"same\"\n", "fixture.toml")
            .expect_err("expected empty cmd validation error");

        assert!(err.contains("cmd must be non-empty"));
    }

    #[test]
    fn rejects_empty_cases() {
        let err = parse_and_validate_suite("cmd = \"x\"\ncases = []\n", "fixture.toml")
            .expect_err("expected empty cases validation error");

        assert!(err.contains("must declare at least one case"));
    }

    #[test]
    fn rejects_duplicate_case_names() {
        let err = parse_and_validate_suite(
            "cmd = \"x\"\n\n[[cases]]\nname = \"dup\"\ninput = \"a\"\n[cases.expect]\nkind = \"same\"\n\n[[cases]]\nname = \"dup\"\ninput = \"b\"\n[cases.expect]\nkind = \"same\"\n",
            "fixture.toml",
        )
        .expect_err("expected duplicate case validation error");

        assert!(err.contains("duplicate case name `dup`"));
    }

    #[test]
    fn rejects_failure_with_same_expectation() {
        let err = parse_and_validate_suite(
            "cmd = \"x\"\n\n[[cases]]\nname = \"bad\"\ninput = \"a\"\nsuccess = false\n[cases.expect]\nkind = \"same\"\n",
            "fixture.toml",
        )
        .expect_err("expected failure+same validation error");

        assert!(err.contains("success=false cannot use expect.kind=same"));
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = parse_and_validate_suite(
            "cmd = \"x\"\n\n[[cases]]\nname = \"bad\"\ninput = \"a\"\nunknown = true\n[cases.expect]\nkind = \"same\"\n",
            "fixture.toml",
        )
        .expect_err("expected unknown field parse error");

        assert!(err.contains("unknown field `unknown`"));
    }
}
