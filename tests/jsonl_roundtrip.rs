#[macro_use]
mod common;

#[test]
fn jsonl_roundtrip() {
    common::run_suite_fixture("tests/test_cases/jsonl_roundtrip.toml");
}
