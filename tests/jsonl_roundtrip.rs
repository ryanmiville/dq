#[macro_use]
mod common;

use common::{dq, run};

table_tests! {
    let dq = dq();

    default {
        cmd: format!("{dq} from jsonl | {dq} to jsonl"),
        success: true,
        stdout: same,
    }

    single_row {
        input: br#"{"name":"Ada","age":37}
"#,
    }

    two_rows {
        input: br#"{"name":"Ada","age":37}
{"name":"Linus","age":54}
"#,
    }

    complex_types {
        input: br#"{"a":1,"b":[1,2],"c":{"x":true},"d":null,"e":"hi"}
"#,
    }

    multiple_scalar {
        input: br#"{"id":1,"score":1.25,"active":false}
{"id":2,"score":0.0,"active":true}
"#,
    }

    empty {
        input: b"",
        stdout: same,
    }

    invalid_input {
        input: br#"not-json
"#,
        success: false,
        stderr_contains: "Malformed JSON",
    }
}
