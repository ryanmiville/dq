use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub fn dq() -> &'static str {
    env!("CARGO_BIN_EXE_dq")
}

pub fn run(command: impl Into<String>) -> Runner {
    Runner {
        command: command.into(),
        stdin: None,
    }
}

pub(crate) fn normalize_output_text(s: &str) -> String {
    s.replace("\r\n", "\n")
        .lines()
        .map(|line| line.trim_end().trim_start_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn assert_normalized_text_eq(left: &str, right: &str) {
    assert_eq!(normalize_output_text(left), normalize_output_text(right));
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Suite {
    cmd: String,
    cases: Vec<Case>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    input: String,
    #[serde(default = "default_success")]
    success: bool,
    expect: Expect,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
enum Expect {
    Same,
    Exact { stdout: String },
    StderrContains { text: String },
}

const fn default_success() -> bool {
    true
}

#[allow(dead_code)]
pub fn run_suite_fixture(fixture_rel_path: &str) {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture_rel_path);
    let suite = load_suite_from_path(&fixture_path);
    let command = suite.cmd.replace("{dq}", dq());

    for case in &suite.cases {
        run_suite_case(&command, &fixture_path, &case);
    }
}

#[allow(dead_code)]
pub fn run_suite_fixture_case(fixture_rel_path: &str, case_name: &str) {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture_rel_path);
    let suite = load_suite_from_path(&fixture_path);
    let command = suite.cmd.replace("{dq}", dq());

    let case = suite
        .cases
        .iter()
        .find(|case| case.name == case_name)
        .unwrap_or_else(|| {
            let names = suite
                .cases
                .iter()
                .map(|case| case.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            panic!(
                "fixture `{}` missing case `{}`. available: [{}]",
                fixture_path.display(),
                case_name,
                names
            )
        });

    run_suite_case(&command, &fixture_path, case);
}

#[allow(dead_code)]
fn run_suite_case(command: &str, fixture_path: &Path, case: &Case) {
    let output = run(command.to_string())
        .stdin(case.input.as_bytes())
        .output()
        .unwrap_or_else(|err| {
            panic!(
                "fixture `{}` case `{}`: failed to run command: {err}",
                fixture_path.display(),
                case.name
            )
        });

    let stderr = String::from_utf8_lossy(&output.stderr);

    if case.success {
        assert!(
            output.status.success(),
            "fixture `{}` case `{}`: expected success, got failure. stderr:\n{}",
            fixture_path.display(),
            case.name,
            stderr
        );
    } else {
        assert!(
            !output.status.success(),
            "fixture `{}` case `{}`: expected failure, got success",
            fixture_path.display(),
            case.name
        );
    }

    match &case.expect {
        Expect::Same => {
            let stdout = std::str::from_utf8(&output.stdout).unwrap_or_else(|err| {
                panic!(
                    "fixture `{}` case `{}`: stdout not valid UTF-8: {err}",
                    fixture_path.display(),
                    case.name
                )
            });
            assert_normalized_text_eq(stdout, &case.input);
        }
        Expect::Exact {
            stdout: expected_stdout,
        } => {
            let stdout = std::str::from_utf8(&output.stdout).unwrap_or_else(|err| {
                panic!(
                    "fixture `{}` case `{}`: stdout not valid UTF-8: {err}",
                    fixture_path.display(),
                    case.name
                )
            });
            assert_normalized_text_eq(stdout, expected_stdout);
        }
        Expect::StderrContains { text } => {
            assert!(
                stderr.contains(text),
                "fixture `{}` case `{}`: stderr missing `{}`. stderr:\n{}",
                fixture_path.display(),
                case.name,
                text,
                stderr
            );
        }
    }
}

#[allow(dead_code)]
fn load_suite_from_path(path: &Path) -> Suite {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("fixture `{}` read error: {err}", path.display()));

    parse_and_validate_suite(&content, &path.display().to_string())
        .unwrap_or_else(|err| panic!("{err}"))
}

fn parse_and_validate_suite(content: &str, fixture_label: &str) -> Result<Suite, String> {
    let suite: Suite = toml::from_str(content)
        .map_err(|err| format!("fixture `{fixture_label}` parse error: {err}"))?;
    validate_suite(&suite, fixture_label)?;
    Ok(suite)
}

fn validate_suite(suite: &Suite, fixture_label: &str) -> Result<(), String> {
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

pub struct Runner {
    command: String,
    stdin: Option<Vec<u8>>,
}

impl Runner {
    pub fn stdin(mut self, input: impl AsRef<[u8]>) -> Self {
        self.stdin = Some(input.as_ref().to_vec());
        self
    }

    pub fn output(self) -> io::Result<Output> {
        let mut command = Command::new("bash");
        command
            .arg("-o")
            .arg("pipefail")
            .arg("-c")
            .arg(self.command);

        if self.stdin.is_some() {
            command.stdin(Stdio::piped());
        }

        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = command.spawn()?;

        if let Some(input) = self.stdin {
            let mut stdin = child.stdin.take().ok_or_else(|| {
                io::Error::new(io::ErrorKind::BrokenPipe, "child stdin unavailable")
            })?;
            stdin.write_all(&input)?;
            drop(stdin);
        }

        child.wait_with_output()
    }
}

macro_rules! __table_tests_apply_fields {
    (
        @parse
        ($cmd:ident, $input:ident, $success:ident, $stdout_mode:ident, $stdout_exact:ident, $stderr_contains:ident)
    ) => {};

    (
        @parse
        ($cmd:ident, $input:ident, $success:ident, $stdout_mode:ident, $stdout_exact:ident, $stderr_contains:ident)
        ,
        $($rest:tt)*
    ) => {
        __table_tests_apply_fields!(
            @parse
            ($cmd, $input, $success, $stdout_mode, $stdout_exact, $stderr_contains)
            $($rest)*
        );
    };

    (
        @parse
        ($cmd:ident, $input:ident, $success:ident, $stdout_mode:ident, $stdout_exact:ident, $stderr_contains:ident)
        cmd: $value:expr,
        $($rest:tt)*
    ) => {
        $cmd = Some(($value).to_string());
        __table_tests_apply_fields!(
            @parse
            ($cmd, $input, $success, $stdout_mode, $stdout_exact, $stderr_contains)
            $($rest)*
        );
    };

    (
        @parse
        ($cmd:ident, $input:ident, $success:ident, $stdout_mode:ident, $stdout_exact:ident, $stderr_contains:ident)
        input: $value:expr,
        $($rest:tt)*
    ) => {
        {
            let __v: &str = $value;
            $input = Some(__v);
        }
        __table_tests_apply_fields!(
            @parse
            ($cmd, $input, $success, $stdout_mode, $stdout_exact, $stderr_contains)
            $($rest)*
        );
    };

    (
        @parse
        ($cmd:ident, $input:ident, $success:ident, $stdout_mode:ident, $stdout_exact:ident, $stderr_contains:ident)
        success: $value:expr,
        $($rest:tt)*
    ) => {
        $success = Some($value);
        __table_tests_apply_fields!(
            @parse
            ($cmd, $input, $success, $stdout_mode, $stdout_exact, $stderr_contains)
            $($rest)*
        );
    };

    (
        @parse
        ($cmd:ident, $input:ident, $success:ident, $stdout_mode:ident, $stdout_exact:ident, $stderr_contains:ident)
        stdout: same,
        $($rest:tt)*
    ) => {
        $stdout_mode = 1;
        $stdout_exact = None;
        __table_tests_apply_fields!(
            @parse
            ($cmd, $input, $success, $stdout_mode, $stdout_exact, $stderr_contains)
            $($rest)*
        );
    };

    (
        @parse
        ($cmd:ident, $input:ident, $success:ident, $stdout_mode:ident, $stdout_exact:ident, $stderr_contains:ident)
        stdout: $value:expr,
        $($rest:tt)*
    ) => {
        $stdout_mode = 2;
        {
            let __v: &str = $value;
            $stdout_exact = Some(__v);
        }
        __table_tests_apply_fields!(
            @parse
            ($cmd, $input, $success, $stdout_mode, $stdout_exact, $stderr_contains)
            $($rest)*
        );
    };

    (
        @parse
        ($cmd:ident, $input:ident, $success:ident, $stdout_mode:ident, $stdout_exact:ident, $stderr_contains:ident)
        stderr_contains: $value:expr,
        $($rest:tt)*
    ) => {
        {
            let __v: &str = $value;
            $stderr_contains = Some(__v);
        }
        __table_tests_apply_fields!(
            @parse
            ($cmd, $input, $success, $stdout_mode, $stdout_exact, $stderr_contains)
            $($rest)*
        );
    };

    (
        @parse
        ($cmd:ident, $input:ident, $success:ident, $stdout_mode:ident, $stdout_exact:ident, $stderr_contains:ident)
        $bad:ident : $value:expr,
        $($rest:tt)*
    ) => {
        compile_error!(concat!("table_tests!: unknown field `", stringify!($bad), "`"));
    };
}

#[cfg(all(test, feature = "common-tests"))]
mod tests {
    use super::parse_and_validate_suite;

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

#[macro_export]
macro_rules! table_tests {
    // Optional setup: only `let ... = ...;` statements
    (@collect_setup [$($setup:tt)*] let $name:ident = $expr:expr; $($rest:tt)*) => {
        table_tests!(@collect_setup [$($setup)* let $name = $expr;] $($rest)*);
    };

    // With defaults
    (@collect_setup [$($setup:tt)*] default { $($defaults:tt)* } $($cases:tt)+) => {
        table_tests!(@emit [$($setup)*] [$($defaults)*] $($cases)+);
    };

    // Without defaults
    (@collect_setup [$($setup:tt)*] $($cases:tt)+) => {
        table_tests!(@emit [$($setup)*] [] $($cases)+);
    };

    // Allow commas between case blocks
    (@emit [$($setup:tt)*] [$($defaults:tt)*] , $($rest:tt)*) => {
        table_tests!(@emit [$($setup)*] [$($defaults)*] $($rest)*);
    };

    // Emit one #[test] per case
    (
        @emit
        [$($setup:tt)*]
        [$($defaults:tt)*]
        $case_name:ident { $($case_fields:tt)* }
        $($rest:tt)*
    ) => {
        #[test]
        fn $case_name() {
            $($setup)*

            let mut __tt_cmd: Option<String> = None;
            let mut __tt_input: Option<&str> = None;
            let mut __tt_success: Option<bool> = None;
            let mut __tt_stdout_mode: u8 = 0; // 0=unset, 1=same, 2=exact
            let mut __tt_stdout_exact: Option<&str> = None;
            let mut __tt_stderr_contains: Option<&str> = None;

            __table_tests_apply_fields!(
                @parse
                (__tt_cmd, __tt_input, __tt_success, __tt_stdout_mode, __tt_stdout_exact, __tt_stderr_contains)
                $($defaults)* ,
            );
            __table_tests_apply_fields!(
                @parse
                (__tt_cmd, __tt_input, __tt_success, __tt_stdout_mode, __tt_stdout_exact, __tt_stderr_contains)
                $($case_fields)* ,
            );

            let __tt_cmd = __tt_cmd.expect("table_tests!: missing required `cmd` (case or default)");
            let __tt_input = __tt_input.expect("table_tests!: missing required `input`");
            let __tt_success = __tt_success.unwrap_or(true);

            if __tt_success {
                assert!(
                    __tt_stdout_mode != 0,
                    "table_tests!: `stdout` is required when success=true (use `stdout: same` or exact text)"
                );
            }

            let __tt_output = run(__tt_cmd)
                .stdin(__tt_input)
                .output()
                .expect("table_tests!: failed to run command");

            let __tt_stderr = String::from_utf8_lossy(&__tt_output.stderr);

            if __tt_success {
                assert!(
                    __tt_output.status.success(),
                    "expected success, got failure.\nstderr:\n{}",
                    __tt_stderr
                );

                match __tt_stdout_mode {
                    1 => {
                        let __tt_stdout_text = std::str::from_utf8(&__tt_output.stdout)
                            .expect("table_tests!: stdout not valid UTF-8");
                        $crate::common::assert_normalized_text_eq(__tt_stdout_text, __tt_input);
                    }
                    2 => $crate::common::assert_normalized_text_eq(
                        std::str::from_utf8(&__tt_output.stdout)
                            .expect("table_tests!: stdout not valid UTF-8"),
                        __tt_stdout_exact.expect("table_tests!: internal missing stdout text"),
                    ),
                    _ => unreachable!(),
                }

                if let Some(__needle) = __tt_stderr_contains {
                    assert!(
                        __tt_stderr.contains(__needle),
                        "stderr did not contain `{}`.\nstderr:\n{}",
                        __needle,
                        __tt_stderr
                    );
                }
            } else {
                assert!(
                    !__tt_output.status.success(),
                    "expected failure, got success"
                );

                if let Some(__needle) = __tt_stderr_contains {
                    assert!(
                        __tt_stderr.contains(__needle),
                        "stderr did not contain `{}`.\nstderr:\n{}",
                        __needle,
                        __tt_stderr
                    );
                } else {
                    assert!(
                        !__tt_output.stderr.is_empty(),
                        "expected non-empty stderr on failure"
                    );
                }
            }
        }

        table_tests!(@emit [$($setup)*] [$($defaults)*] $($rest)*);
    };

    (@emit [$($setup:tt)*] [$($defaults:tt)*]) => {};

    // public entrypoint LAST
        ($($all:tt)*) => {
            table_tests!(@collect_setup [] $($all)*);
        };
}

// ---- Example usage ----
