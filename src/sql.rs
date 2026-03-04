// TODO: query builder to remove boilerplate for each command
// from:
// COPY (SELECT * FROM read_json_auto('/dev/stdin', format='newline_delimited')) TO '/dev/stdout' (FORMAT ARROW);

// to:
// CREATE TEMP TABLE dq_input AS SELECT {columns} FROM read_arrow('/dev/stdin');
// COPY dq_input TO '/dev/stdout' (FORMAT ARROW);

pub fn cmd_from(data_type: &DataType) -> &str {
    let read_fn = match data_type {
        DataType::Jsonl => "read_json_auto('/dev/stdin', format='newline_delimited')",
        DataType::Json => "read_json_auto('/dev/stdin')",
        DataType::Arrow => "read_arrow('/dev/stdin')",
    };

    format!("COPY (SELECT * FROM {read_fn}) TO '/dev/stdout' (FORMAT ARROW);")
}

pub fn cmd_to(data_type: &DataType) {
    let ctt = create_temp_table("*");
    let copy = copy_temp_table(data_type);
    format!("{ctt} {copy}")
}

pub fn cmd_select(columns: &str) -> &str {
    let ctt = create_temp_table(columns, "");
    let copy = copy_temp_table(DataType::Arrow);
    format!("{ctt} {copy}")
}

pub fn cmd_where(clause: &str) -> &str {
    let ctt = create_temp_table("*", clause);
    let copy = copy_temp_table(DataType::Arrow);
    format!("{ctt} {copy}")
}

fn create_temp_table(columns: &str, where_clause: &str) -> &str {
    if where_clause.is_empty() {
        format!("CREATE TEMP TABLE dq_input AS SELECT {columns} FROM read_arrow('/dev/stdin');")
    } else {
        format!(
            "CREATE TEMP TABLE dq_input AS SELECT {columns} FROM read_arrow('/dev/stdin') WHERE {where_clause};"
        )
    }
}

fn copy_temp_table(data_type: &DataType) -> &str {
    let fmt = match data_type {
        DataType::Jsonl => "(FORMAT JSON, ARRAY false)",
        DataType::Json => "(FORMAT JSON, ARRAY true)",
        DataType::Arrow => "(FORMAT ARROW)",
    };

    format!("COPY dq_input TO '/dev/stdout' {fmt};")
}
