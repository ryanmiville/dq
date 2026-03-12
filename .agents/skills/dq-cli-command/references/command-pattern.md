# Command pattern

Use `select` and `where` as the canonical pattern for a transform stage.

## Files to touch

- `src/main.rs`
- `src/cmd.rs`
- `tests/test_cases/<command>.toml`

## Pattern in `src/main.rs`

1. Add a `Command` enum variant with:
   - doc comment shown in `--help`
   - one positional string argument for the SQL fragment, if command matches `select` / `where`
2. Update `run()` match to call `cmd::<handler>(&conn, &arg)`.

Example shape:

```rust
/// Describe the command in pipeline terms
NewCommand {
    /// Describe the SQL fragment or argument
    expr: String,
}
```

Dispatch shape:

```rust
Command::NewCommand { expr } => cmd::new_command(&conn, &expr),
```

## Pattern in `src/cmd.rs`

For Arrow-in transform stages, follow this exact structure:

```rust
pub fn new_command(conn: &Connection, expr: &str) -> Result<()> {
    let query = format!("SELECT ... FROM read_arrow('/dev/stdin') ... {expr}");
    emit_relation_query(conn, &query).context("failed to ...")
}
```

## Why this pattern

- Keeps stdin/stdout handling centralized
- Preserves pretty output when stdout is a terminal
- Minimizes command-specific code
- Matches existing repo conventions

## Prefer this over alternatives

- Prefer `emit_relation_query()` over direct `COPY` unless command is a sink/source like `to`/`from`
- Prefer one focused handler function over generic abstraction
- Prefer command-specific error context strings like `failed to filter data`

## Checklist

- Add clap docs for the command and its argument
- Add match arm in `run()`
- Add handler in `src/cmd.rs`
- Use `read_arrow('/dev/stdin')` for Arrow-consuming stages
- Route through `emit_relation_query()`
- Add `.context(...)`
