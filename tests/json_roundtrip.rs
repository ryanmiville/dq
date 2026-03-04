mod common;

#[test]
fn json_roundtrip() {
    common::run_suite_fixture("tests/test_cases/json_roundtrip.toml");
}
