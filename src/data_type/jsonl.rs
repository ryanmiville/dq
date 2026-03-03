pub const FROM_SQL: &str = "COPY (SELECT * FROM read_json_auto('/dev/stdin', format='newline_delimited')) TO '/dev/stdout' (FORMAT ARROW);";

pub const TO_SQL: &str = "CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin'); COPY dq_input TO '/dev/stdout' (FORMAT JSON, ARRAY false);";
