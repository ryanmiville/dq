mod common;

#[test]
fn supports_raw_copy_options_for_to_command() {
    let input = "name,age\nAda,37\nLinus,54\n";

    let output = common::run(format!(
        "{} from csv | {} to \"(FORMAT CSV, DELIMITER '|', HEADER)\"",
        common::dq(),
        common::dq()
    ))
    .stdin(input)
    .output()
    .expect("failed to run dq pipeline");

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = std::str::from_utf8(&output.stdout).expect("stdout is not utf-8");
    common::assert_normalized_text_eq(stdout, "name|age\nAda|37\nLinus|54\n");
}
