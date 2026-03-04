#[macro_use]
mod common;

use common::{dq, run};

table_tests! {
    let dq = dq();

    default {
        cmd: format!("{dq} from json | {dq} to json"),
        success: true,
        stdout: same,
    }

    single_row {
        input: r#"[
        {"name":"Ada","age":37}
]
"#,
    }

    two_rows {
        input: r#"[
    {"name":"Ada","age":37},
    {"name":"Linus","age":54}
]
"#,
    }

    complex_types {
        input: r#"[
    {"a":1,"b":[1,2],"c":{"x":true},"d":null,"e":"hi"}
]
"#,
    }

    multiple_scalar {
        input: r#"[
    {"id":1,"score":1.25,"active":false},
    {"id":2,"score":0.0,"active":true}
]
"#,
    }

    empty {
        input: "",
        stdout: "[

]",
    }

    invalid_input {
        input: r#"not-json
"#,
        success: false,
        stderr_contains: "Malformed JSON",
    }

}
// #[test]
// fn json_roundtrip_and_invalid_input() {
//     let dq = dq();
//     let roundtrip_cases: [&[u8]; 3] = [
//         b"[\n\t{\"name\":\"Ada\",\"age\":37}\n]\n",
//         b"[\n\t{\"name\":\"Ada\",\"age\":37},\n\t{\"name\":\"Linus\",\"age\":54}\n]\n",
//         b"[\n\t{\"a\":1,\"b\":[1,2],\"c\":{\"x\":true},\"d\":null,\"e\":\"hi\"}\n]\n",
//     ];

//     for case in roundtrip_cases {
//         let cmd = format!("{dq} from json | {dq} to json");
//         let output = run(cmd).stdin(case).output().expect("run dq pipeline");

//         assert!(
//             output.status.success(),
//             "dq json pipeline failed: {}",
//             String::from_utf8_lossy(&output.stderr)
//         );
//         assert_eq!(output.stdout, case);
//     }

//     let invalid_output = run(format!("{dq} from json"))
//         .stdin(br#"[{"name":"Ada"}"#)
//         .output()
//         .expect("run dq invalid command");

//     assert!(!invalid_output.status.success());
//     assert!(!invalid_output.stderr.is_empty());
// }
