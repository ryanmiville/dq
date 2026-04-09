# Command pattern

Use `select`, `where`, `limit`, `order-by`, `describe`, and `summarize` as the canonical pattern for a transform stage.

## Files to touch

Usually:

- `src/main.rs`
- `src/cmd.rs`
- `src/plan.rs`
- `tests/test_cases/<command>.toml`

## Pattern in `src/main.rs`

1. Add a `Command` enum variant with:
   - doc comment shown in `--help`
   - one positional string argument for the SQL fragment, if command matches `select` / `where` / `order-by` / `limit`
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

## Pattern in `src/plan.rs`

If the command changes query semantics, add one `Op` variant and one compiler lowering arm.

Example shape:

```rust
pub enum Op {
    // ...
    NewCommand { expr: String },
}
```

Compiler shape:

```rust
Op::NewCommand { expr } => format!("SELECT * FROM ({input}) AS q ... {expr}"),
```

Keep compiler output relation-shaped. If the op is metadata-like, lower it to a selectable relation, e.g. `SELECT * FROM (DESCRIBE ...) AS q`, not a bare statement that cannot be wrapped by `COPY`.

## Pattern in `src/cmd.rs`

For plan-transform stages, follow this exact structure:

```rust
pub fn new_command(conn: &Connection, expr: &str) -> Result<()> {
    let plan = Plan::read_from(io::stdin().lock())
        .context("failed to read input plan")?
        .with_op(Op::NewCommand {
            expr: expr.to_string(),
        });
    emit_plan_or_pretty(conn, &plan).context("failed to ...")
}
```

For no-arg commands, same pattern without the payload:

```rust
pub fn describe_like(conn: &Connection) -> Result<()> {
    let plan = Plan::read_from(io::stdin().lock())
        .context("failed to read input plan")?
        .with_op(Op::DescribeLike);
    emit_plan_or_pretty(conn, &plan).context("failed to ...")
}
```

## Why this pattern

- Keeps stdin/stdout plan handling centralized
- Preserves tty auto-execution and pretty output
- Minimizes command-specific code
- Matches existing repo conventions
- Keeps ordering semantics explicit in the compiler

## Prefer this over alternatives

- Prefer `emit_plan_or_pretty()` over duplicating tty branching
- Prefer one focused handler function over generic abstraction
- Prefer one `Op` per semantic pipeline step
- Prefer command-specific error context strings like `failed to filter data`

## Checklist

- Add clap docs for the command and its argument
- Add match arm in `run()`
- Add handler in `src/cmd.rs`
- Add or update `Op` in `src/plan.rs` if needed
- Add compiler lowering in `compile_op()`
- Preserve ordered semantics by nesting over prior query text
- Route through `emit_plan_or_pretty()`
- Add `.context(...)`
- Add or update fixtures
