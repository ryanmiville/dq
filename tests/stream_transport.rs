mod common;

use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

const STREAM_MAGIC: &[u8; 4] = b"DQSP";
const STREAM_VERSION: u32 = 1;
const PREFIX_LEN: usize = 12;

#[test]
fn from_streams_framed_plan_followed_by_raw_input() {
    let input = b"{\"name\":\"Ada\",\"age\":37}\n{\"name\":\"Linus\",\"age\":54}\n";

    let output = common::run(format!("{} from json", common::dq()))
        .stdin(input)
        .output()
        .unwrap();

    assert_success(&output, "from");
    let (plan, payload) = decode_stream(&output.stdout);
    assert_eq!(plan["version"], 1);
    assert_eq!(plan["source"]["kind"], "stream");
    assert_eq!(plan["source"]["read_expr"], "read_json_auto('/dev/stdin')");
    assert_eq!(payload, input);
}

#[test]
fn intermediate_stage_updates_plan_and_preserves_binary_payload() {
    let payload = b"\0raw\xffbytes\n";
    let source = common::run(format!("{} from csv", common::dq()))
        .stdin(payload)
        .output()
        .unwrap();
    assert_success(&source, "from");

    let transformed = common::run(format!("{} limit 1", common::dq()))
        .stdin(&source.stdout)
        .output()
        .unwrap();

    assert_success(&transformed, "limit");
    let (plan, actual_payload) = decode_stream(&transformed.stdout);
    assert_eq!(plan["ops"][0]["kind"], "limit");
    assert_eq!(plan["ops"][0]["count"], "1");
    assert_eq!(actual_payload, payload);
}

#[test]
fn endpoint_handoff_preserves_first_payload_bytes() {
    let input = "{\"name\":\"Ada\",\"age\":37}\n{\"name\":\"Linus\",\"age\":54}\n";

    let output = common::run(format!(
        "{} from json | {} where \"age > 40\" | {} select name | {} to json",
        common::dq(),
        common::dq(),
        common::dq(),
        common::dq()
    ))
    .stdin(input)
    .output()
    .unwrap();

    assert_success(&output, "pipeline");
    common::assert_normalized_text_eq(
        std::str::from_utf8(&output.stdout).unwrap(),
        "{\"name\":\"Linus\"}\n",
    );
}

#[test]
fn encoded_stream_can_be_redirected_from_a_regular_file() {
    let input = b"{\"name\":\"Ada\",\"age\":37}\n{\"name\":\"Linus\",\"age\":54}\n";
    let encoded = common::run(format!(
        "{} from json | {} where \"age > 40\"",
        common::dq(),
        common::dq()
    ))
    .stdin(input)
    .output()
    .unwrap();
    assert_success(&encoded, "encoded pipeline");

    let path = unique_temp_path("dq-stream", "bin");
    fs::write(&path, &encoded.stdout).unwrap();
    let output = common::run(format!("{} to json < {}", common::dq(), shell_quote(&path)))
        .output()
        .unwrap();
    fs::remove_file(&path).unwrap();

    assert_success(&output, "redirected endpoint");
    common::assert_normalized_text_eq(
        std::str::from_utf8(&output.stdout).unwrap(),
        "{\"name\":\"Linus\",\"age\":54}\n",
    );
}

#[test]
fn large_payload_crosses_buffer_boundaries_without_corruption() {
    let mut input = String::new();
    for index in 0..10_000 {
        input.push_str(&format!(
            "{{\"index\":{index},\"value\":\"row-{index}\"}}\n"
        ));
    }

    let input_path = unique_temp_path("dq-large-input", "json");
    let output_path = unique_temp_path("dq-large-output", "json");
    fs::write(&input_path, input.as_bytes()).unwrap();

    let output = common::run(format!(
        "{} from json < {} | {} where \"index >= 9998\" | {} to json > {}",
        common::dq(),
        shell_quote(&input_path),
        common::dq(),
        common::dq(),
        shell_quote(&output_path)
    ))
    .output()
    .unwrap();
    let actual = fs::read_to_string(&output_path).unwrap();
    fs::remove_file(&input_path).unwrap();
    fs::remove_file(&output_path).unwrap();

    assert_success(&output, "large pipeline");
    common::assert_normalized_text_eq(
        &actual,
        "{\"index\":9998,\"value\":\"row-9998\"}\n{\"index\":9999,\"value\":\"row-9999\"}\n",
    );
}

#[test]
fn early_downstream_close_is_normal_pipeline_completion() {
    let input_path = unique_temp_path("dq-early-close", "json");
    let mut input = String::new();
    for index in 0..100_000 {
        input.push_str(&format!("{{\"index\":{index}}}\n"));
    }
    fs::write(&input_path, input).unwrap();

    let output = common::run(format!(
        "{} from json < {} | {} where \"index >= 0\" | {} to json | head -n 1",
        common::dq(),
        shell_quote(&input_path),
        common::dq(),
        common::dq()
    ))
    .output()
    .unwrap();
    fs::remove_file(&input_path).unwrap();

    assert_success(&output, "early-closing pipeline");
    common::assert_normalized_text_eq(
        std::str::from_utf8(&output.stdout).unwrap(),
        "{\"index\":0}\n",
    );
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn file_source_remains_user_owned() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/people.json");

    let output = common::run(format!(
        "{} from tests/data/people.json | {} to json",
        common::dq(),
        common::dq()
    ))
    .output()
    .unwrap();

    assert_success(&output, "file pipeline");
    assert!(source_path.exists(), "file-backed source was deleted");
}

fn decode_stream(bytes: &[u8]) -> (Value, &[u8]) {
    assert!(bytes.len() >= PREFIX_LEN, "missing stream prefix");
    assert_eq!(&bytes[..4], STREAM_MAGIC);
    assert_eq!(
        u32::from_be_bytes(bytes[4..8].try_into().unwrap()),
        STREAM_VERSION
    );

    let header_len = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let payload_offset = PREFIX_LEN.checked_add(header_len).unwrap();
    assert!(
        bytes.len() >= payload_offset,
        "declared header exceeds stream length"
    );

    let plan = serde_json::from_slice(&bytes[PREFIX_LEN..payload_offset]).unwrap();
    (plan, &bytes[payload_offset..])
}

fn assert_success(output: &std::process::Output, command: &str) {
    assert!(
        output.status.success(),
        "{command} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn unique_temp_path(prefix: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}.{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test"),
        extension
    ))
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}
