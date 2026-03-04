use std::io::{self, Write};
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
            let __v: &[u8] = $value;
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
            let mut __tt_input: Option<&[u8]> = None;
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
                    "table_tests!: `stdout` is required when success=true (use `stdout: same` or exact bytes)"
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
                        let __tt_input_text =
                            std::str::from_utf8(__tt_input).expect("table_tests!: input not valid UTF-8");
                        $crate::common::assert_normalized_text_eq(__tt_stdout_text, __tt_input_text);
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
