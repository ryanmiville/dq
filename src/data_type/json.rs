pub const TO_SQL: &str = "CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin'); COPY dq_input TO '/dev/stdout' (FORMAT JSON, ARRAY true);";

pub const FROM_SQL: &str =
    "COPY (SELECT * FROM read_json_auto('/dev/stdin')) TO '/dev/stdout' (FORMAT ARROW);";
