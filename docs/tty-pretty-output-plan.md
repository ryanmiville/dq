# Pretty output implementation plan

## Summary

Implement pretty-table rendering in two phases.

### Phase 1: explicit pretty sink

Add:

- `dq to pretty`

Behavior:

- reads Arrow from `stdin`
- renders a simple pretty table to `stdout`
- uses the default minimal Arrow pretty-printer output

This gives us a deterministic, easy-to-test pretty-print path without needing TTY simulation.

### Phase 2: TTY-aware auto display

After `to pretty` is working and tested, make relation-producing commands auto-render pretty tables when writing directly to a terminal:

- `dq from ...`
- `dq select ...`
- `dq where ...`

Behavior in phase 2:

- `stdout` is a TTY -> pretty table
- `stdout` is not a TTY -> Arrow, exactly as today

This keeps the current Arrow pipeline architecture intact while giving interactive users a better default experience.

---

## Why this phased approach is better

The earlier approach bundled two changes together:

1. adding pretty output
2. adding TTY/auto routing

That makes testing and implementation harder than necessary.

By starting with `dq to pretty`, we get:

- a clean, explicit user-facing feature
- a reusable pretty-output code path
- fixture-based tests using the existing harness unchanged
- manual TTY validation later, once the renderer itself is already proven

This is the lowest-risk path.

---

## Current state

Today the command model is:

- `from` reads raw data and emits Arrow to `stdout`
- `select` reads Arrow from `stdin`, transforms it, and emits Arrow to `stdout`
- `where` reads Arrow from `stdin`, transforms it, and emits Arrow to `stdout`
- `to` reads Arrow from `stdin` and writes a concrete output format to `stdout`

This is already a strong architecture:

- Arrow is the inter-stage protocol
- `to` is the explicit sink/serialization command

The new pretty behavior should build on that rather than bypassing it.

---

## Phase 1: add `dq to pretty`

## Goal

Extend `to` so that it supports a new format preset:

- `pretty`

Example:

```bash
echo '{"name":"Ada","age":37}' |
  dq from jsonl |
  dq to pretty
```

Expected behavior:

- output is a plain ASCII table from Arrow's default pretty formatter
- no custom styling or terminal-specific behavior yet

---

## Technical design for phase 1

## 1. Extend the format model

### `src/format.rs`

Today `Format` distinguishes between preset formats and passthrough strings.

Add a new preset variant:

```rust
Pretty
```

So presets become something like:

- `Csv`
- `Json`
- `Jsonl`
- `Pretty`

### Behavioral rule

- `pretty` is valid for `to`
- `pretty` is **not** valid for `from`

Because `from pretty` does not make sense as an input reader.

### Recommended handling

Do not try to make `pretty` fit into `read_fn()`.

Instead, treat `pretty` as a special-case sink in `cmd::to()`.

That keeps the model simple:

- normal `to` presets use `COPY ...`
- `to pretty` uses an in-process renderer

---

## 2. Special-case `to pretty` in `src/cmd.rs`

### Current behavior of `to()`

Right now `to()` always does roughly:

```sql
CREATE TEMP TABLE dq_input AS SELECT * FROM read_arrow('/dev/stdin');
COPY dq_input TO '/dev/stdout' {copy_format};
```

### New behavior

Split `to()` into two paths:

- `to pretty` -> pretty-render path
- all other formats -> existing `COPY` path unchanged

### Suggested structure

```rust
pub fn to(conn: &Connection, format: &Format) -> Result<()> {
    match format {
        Format::Preset(Preset::Pretty) => to_pretty(conn),
        _ => to_copy_format(conn, format),
    }
}
```

Or equivalent naming.

### Pretty path

For `to pretty`:

1. read Arrow input via DuckDB query
2. collect Arrow `RecordBatch` values through `duckdb-rs`
3. format them with Arrow's pretty formatter
4. print to stdout

Conceptually:

```rust
let mut stmt = conn.prepare("SELECT * FROM read_arrow('/dev/stdin')")?;
let batches: Vec<RecordBatch> = stmt.query_arrow([])?.collect();
let table = pretty_format_batches(&batches)?;
println!("{table}");
```

### Important note

This path should not use `COPY`.

`COPY` is still the correct mechanism for machine-oriented serialization formats. Pretty output is a display concern, so it should stay in Rust-land after the query result is obtained.

---

## 3. Use Arrow's default pretty-printer exactly as-is

For the first iteration, keep the renderer minimal.

### Requirements

- no custom formatting logic
- no width truncation logic
- no ANSI colors
- no pager integration
- no row count footer
- no attempt to mimic DuckDB CLI formatting exactly

Use the Arrow formatter available through the current dependency stack.

Likely API shape:

```rust
duckdb::arrow::util::pretty::print_batches
```

or

```rust
pretty_format_batches
```

Depending on what is easiest with the currently re-exported version.

If possible, prefer the formatter that returns a string/displayable value rather than directly printing, because it is easier to control exact stdout behavior.

---

## 4. Keep all existing non-pretty behavior unchanged

This is critical.

### `dq to csv`, `dq to json`, `dq to jsonl`

These should behave exactly as they do today.

### `dq from`, `dq select`, `dq where`

These should also behave exactly as they do today in phase 1.

That means:

- they still emit Arrow
- no TTY detection yet
- no auto pretty behavior yet

This minimizes scope and regression risk.

---

## Phase 1 file-by-file plan

## `src/format.rs`

### Changes

- add `Pretty` to `Preset`
- keep `copy_format()` unchanged for the existing real serialization formats
- avoid forcing `pretty` through `copy_format()` if that makes the API awkward

### Recommendation

If needed, update helpers so `pretty` is handled outside of `copy_format()` entirely.

For example, `copy_format()` may become partial in practice:

- only called for formats that actually map to DuckDB `COPY` options

That is fine.

---

## `src/cmd.rs`

### Changes

Refactor `to()` into:

- normal copy-based output path
- pretty-render path

Suggested helper split:

- `to_copy_format(conn, format)`
- `to_pretty(conn)`

### `to_pretty(conn)` responsibilities

- run `SELECT * FROM read_arrow('/dev/stdin')`
- collect batches
- pretty format them
- write to stdout

---

## `src/main.rs`

### Changes

Probably none beyond whatever is required by the format enum update.

The existing dispatch can remain:

```rust
Command::To { format } => cmd::to(&conn, &Format::parse(format))
```

---

## Phase 1 test plan

## Goal

Test pretty output using the exact same fixture harness already in the repo.

Because `dq to pretty` is explicit, we do **not** need to simulate a TTY.

This avoids new env plumbing, pseudo-terminals, or test harness changes.

---

## A. Add a new pretty-output fixture by copying an existing roundtrip test

Per the agreed direction, copy one of the current roundtrip fixtures and change the command so the final sink is `to pretty`.

Best candidate:

- copy `tests/test_cases/json_roundtrip.toml`
- create `tests/test_cases/json_pretty.toml`

Change:

```toml
cmd = "{dq} from json | {dq} to pretty"
```

Then update expectations from `same` to `exact` using the formatter's real output.

### Why this is ideal

- same fixture style as everything else
- deterministic captured stdout text
- no new test framework features needed

---

## B. Use exact expected stdout

The new pretty test should use:

```toml
[cases.expect]
kind = "exact"
stdout = """
...
"""
```

Populate the expected text with the actual Arrow pretty-printer output.

Because the project already normalizes leading indentation and line endings in tests, this should be straightforward.

---

## C. Keep existing fixtures untouched

All current fixtures should continue passing unchanged.

That confirms:

- Arrow pipeline behavior is preserved
- normal `to csv/json/jsonl` output is unchanged
- adding `pretty` did not disturb existing functionality

---

## D. Optional extra fixture later

If useful, add a second pretty-output fixture for CSV:

- `tests/test_cases/csv_pretty.toml`
- `cmd = "{dq} from csv | {dq} to pretty"`

But for the initial iteration, one JSON-based pretty fixture is enough.

---

## Phase 1 edge cases to accept

These are acceptable for the initial `to pretty` implementation:

- full batch collection in memory before formatting
- large outputs printing the entire table
- formatter differences from the DuckDB CLI
- empty input using whatever Arrow pretty-printer emits for an empty result

The point of v1 is correctness and architecture, not presentation polish.

---

## Phase 2: add TTY-aware auto pretty output

Once `dq to pretty` exists and is well-tested, add automatic pretty display for relation-producing commands.

## Desired phase 2 behavior

- `dq from ...` writing directly to terminal -> pretty table
- `dq select ...` writing directly to terminal -> pretty table
- `dq where ...` writing directly to terminal -> pretty table
- same commands writing to a pipe -> Arrow

Example:

```bash
printf '{"name":"Ada","age":37}\n' | dq from jsonl
```

Should display a pretty table when run directly in a terminal.

But:

```bash
printf '{"name":"Ada","age":37}\n' | dq from jsonl | dq select "name"
```

Should still exchange Arrow between the commands.

---

## Phase 2 technical direction

## 1. Introduce output mode routing

Suggested enum:

```rust
enum OutputMode {
    Auto,
    Arrow,
    Pretty,
}
```

`Auto` resolution:

- `stdout` is terminal -> `Pretty`
- otherwise -> `Arrow`

Use:

```rust
use std::io::IsTerminal;
std::io::stdout().is_terminal()
```

---

## 2. Refactor relation-producing commands to build queries and emit via a shared helper

For:

- `from`
- `select`
- `where`

build a query string first, then choose the output path.

Examples:

- `from`: `SELECT * FROM {read_fn}`
- `select`: `SELECT {columns} FROM read_arrow('/dev/stdin')`
- `where`: `SELECT * FROM read_arrow('/dev/stdin') WHERE {clause}`

Then route through a shared emitter:

- Arrow path -> `COPY ({query}) TO '/dev/stdout' (FORMAT ARROW)`
- pretty path -> same query rendered through the same logic as `to pretty`

This is where `to pretty` pays off: it becomes the already-proven display implementation.

---

## 3. Manual verification plan for phase 2

Because the initial goal is to avoid extra test harness complexity, phase 2 terminal behavior can be manually verified first.

Suggested manual checks:

### Terminal output

```bash
printf '{"name":"Ada","age":37}\n' | dq from jsonl
```

Expected:

- pretty table displayed

### Pipeline output remains Arrow

```bash
printf '{"name":"Ada","age":37}\n' | dq from jsonl | dq to jsonl
```

Expected:

- JSONL output identical to today

### Mid-pipeline commands remain composable

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' |
  dq from jsonl |
  dq where "age >= 40" |
  dq to pretty
```

Expected:

- pretty output at final sink

and:

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' |
  dq from jsonl |
  dq where "age >= 40" |
  dq select "name" |
  dq to jsonl
```

Expected:

- Arrow exchange internally, JSONL at the end

---

## Future optional improvements

After both phases are complete, possible follow-ups include:

- `--output pretty|arrow|auto`
- `DQ_OUTPUT=pretty|arrow|auto`
- width-aware truncation
- pager support
- row count footer
- closer parity with DuckDB CLI table formatting

None of these are required for the current milestone.

---

## Suggested implementation order

### Phase 1

1. Add `Pretty` preset support
2. Implement `to pretty` in `src/cmd.rs`
3. Add a copied roundtrip fixture that becomes `... | dq to pretty`
4. Capture and commit exact pretty-printer output in the fixture
5. Run full test suite

### Phase 2

6. Add output-mode routing for relation-producing commands
7. Reuse the same pretty-render helper from `to pretty`
8. Manually verify terminal-vs-pipe behavior
9. Add automated TTY tests only if later needed

---

## Exit criteria

### Phase 1 is complete when

- `dq to pretty` exists
- it reads Arrow from stdin and prints a table to stdout
- the output uses the default minimal Arrow pretty-printer
- one new fixture-based test verifies the exact pretty output
- all existing tests still pass unchanged

### Phase 2 is complete when

- `dq from`, `dq select`, and `dq where` auto-render pretty tables on a real terminal
- those same commands still emit Arrow when piped
- manual verification confirms terminal-vs-pipe behavior

---

## Recommendation

Proceed by implementing `dq to pretty` first and testing it with the existing fixture harness.

Then, once the rendering path is stable, add TTY-aware auto pretty output by routing relation-producing commands through the same pretty renderer when `stdout` is a terminal.
