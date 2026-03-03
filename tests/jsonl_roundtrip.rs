use std::io::{Read, Write};
use std::process::{Command, Stdio};

fn run_from_to_jsonl(input: &[u8]) -> (std::process::Output, std::process::ExitStatus, Vec<u8>) {
    let bin = env!("CARGO_BIN_EXE_dq");

    let mut from = Command::new(bin)
        .args(["from", "jsonl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn dq from jsonl");

    let mut from_stdin = from.stdin.take().expect("from stdin");
    from_stdin.write_all(input).expect("write test input");
    drop(from_stdin);

    let from_stdout = from.stdout.take().expect("from stdout");
    let mut from_stderr = from.stderr.take().expect("from stderr");

    let to_output = Command::new(bin)
        .args(["to", "jsonl"])
        .stdin(Stdio::from(from_stdout))
        .output()
        .expect("run dq to jsonl");

    let mut from_stderr_output = Vec::new();
    from_stderr
        .read_to_end(&mut from_stderr_output)
        .expect("read from stderr");

    let from_status = from.wait().expect("wait for from command");

    (to_output, from_status, from_stderr_output)
}

#[test]
fn jsonl_roundtrip_and_invalid_input() {
    let roundtrip_cases: [&[u8]; 5] = [
        br#"{"name":"Ada","age":37}
"#,
        br#"{"name":"Ada","age":37}
{"name":"Linus","age":54}
"#,
        br#"{"a":1,"b":[1,2],"c":{"x":true},"d":null,"e":"hi"}
"#,
        br#"{"id":1,"score":1.25,"active":false}
{"id":2,"score":0.0,"active":true}
"#,
        b"",
    ];

    for case in roundtrip_cases {
        let (to_output, from_status, from_stderr) = run_from_to_jsonl(case);

        assert!(
            from_status.success(),
            "dq from jsonl failed: {}",
            String::from_utf8_lossy(&from_stderr)
        );
        assert!(
            to_output.status.success(),
            "dq to jsonl failed: {}",
            String::from_utf8_lossy(&to_output.stderr)
        );
        assert_eq!(to_output.stdout, case);
    }

    let bin = env!("CARGO_BIN_EXE_dq");
    let mut invalid = Command::new(bin)
        .args(["from", "jsonl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn dq from jsonl invalid test");

    let mut invalid_stdin = invalid.stdin.take().expect("invalid stdin");
    invalid_stdin
        .write_all(
            br#"{"name":"Ada"}
not-json
"#,
        )
        .expect("write invalid input");
    drop(invalid_stdin);

    let invalid_output = invalid.wait_with_output().expect("wait invalid command");

    assert!(!invalid_output.status.success());
    assert!(!invalid_output.stderr.is_empty());
}
