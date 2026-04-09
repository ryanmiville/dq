mod common;

use serde_json::Value;
use std::path::{Path, PathBuf};

#[test]
fn to_cleans_up_materialized_source_dir() {
    let input = "{\"name\":\"Ada\",\"age\":37}\n{\"name\":\"Linus\",\"age\":54}\n";

    let plan_output = common::run(format!("{} from json", common::dq()))
        .stdin(input)
        .output()
        .unwrap();

    assert!(
        plan_output.status.success(),
        "from failed: {}",
        String::from_utf8_lossy(&plan_output.stderr)
    );

    let plan: Value = serde_json::from_slice(&plan_output.stdout).unwrap();
    let source_path = PathBuf::from(plan["source_path"].as_str().unwrap());
    let source_dir = source_path.parent().unwrap().to_path_buf();

    assert!(
        source_path.exists(),
        "materialized source missing: {}",
        source_path.display()
    );
    assert!(
        source_dir.join(".dq-owned").exists(),
        "ownership marker missing: {}",
        source_dir.display()
    );

    let output = common::run(format!("{} to json", common::dq()))
        .stdin(&plan_output.stdout)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "to failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    common::assert_normalized_text_eq(stdout, input);
    assert!(
        !source_dir.exists(),
        "temp source dir still exists: {}",
        source_dir.display()
    );
}

#[test]
fn to_does_not_delete_file_source() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/people.json");
    assert!(
        source_path.exists(),
        "fixture source missing: {}",
        source_path.display()
    );

    let output = common::run(format!(
        "{} from tests/data/people.json | {} to json",
        common::dq(),
        common::dq()
    ))
    .output()
    .unwrap();

    assert!(
        output.status.success(),
        "pipeline failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    common::assert_normalized_text_eq(
        stdout,
        "{\"name\":\"Ada\",\"age\":37}\n{\"name\":\"Linus\",\"age\":54}\n",
    );
    assert!(
        source_path.exists(),
        "file-backed source was deleted: {}",
        source_path.display()
    );
}
