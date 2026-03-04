use dq_test_fixtures::{Case, Expect};
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
pub fn run_suite_fixture(fixture_rel_path: &str) {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture_rel_path);
    let suite =
        dq_test_fixtures::load_suite_from_path(&fixture_path).unwrap_or_else(|err| panic!("{err}"));
    let command = suite.cmd.replace("{dq}", dq());

    for case in &suite.cases {
        run_suite_case(&command, &fixture_path, &case);
    }
}

#[allow(dead_code)]
pub fn run_suite_fixture_case(fixture_rel_path: &str, case_name: &str) {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture_rel_path);
    let suite =
        dq_test_fixtures::load_suite_from_path(&fixture_path).unwrap_or_else(|err| panic!("{err}"));
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
