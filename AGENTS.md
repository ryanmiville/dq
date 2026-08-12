# PROJECT KNOWLEDGE BASE

**Generated:** 2026-03-08
**Commit:** 83c55bb
**Branch:** main

## OVERVIEW

`dq` — small Rust CLI for shell-first data pipelines powered by DuckDB. Pipe-composes `from`, `to`, `select`, `where` subcommands; stages exchange a framed plan plus raw streamed input over stdin/stdout.

## STRUCTURE

```
dq/
├── src/
│   ├── main.rs         # CLI entry, clap parsing, DuckDB connection setup
│   ├── cmd.rs          # Subcommand implementations (from/to/select/where)
│   ├── format.rs       # Format enum: presets (csv/json/json-array) + raw passthrough
│   ├── plan.rs         # Deferred source and operation plan + SQL compilation
│   └── stream.rs       # Private framing protocol and exact stdin handoff
├── dq_test_macros/     # Proc-macro crate: generates #[test] fns from TOML fixtures
├── dq_test_fixtures/   # Fixture schema (Suite/Case/Expect) + TOML parsing/validation
└── tests/
    ├── fixtures.rs     # Single entry: fixture_tests_dir!("tests/test_cases")
    ├── common/mod.rs   # Test runner: shell pipeline execution + output assertion
    └── test_cases/     # TOML fixture files (one per command/scenario)
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add new subcommand | `src/main.rs` (Command enum), `src/cmd.rs` | Add clap variant + handler fn |
| Add format preset | `src/format.rs` (Preset enum) | Implement `read_fn()` + `copy_format()` |
| Add test cases | `tests/test_cases/*.toml` | Auto-discovered by proc-macro |
| Change test fixture schema | `dq_test_fixtures/src/lib.rs` | Suite/Case/Expect types + validation |
| Change test codegen | `dq_test_macros/src/lib.rs` | Proc-macro: fixture_tests! / fixture_tests_dir! |
| Change test execution | `tests/common/mod.rs` | Runner, assertion logic |

## CONVENTIONS

- **Workspace layout**: 3 crates — binary (`dq`), proc-macro (`dq_test_macros`), library (`dq_test_fixtures`)
- **Pipeline protocol**: A fixed-size binary prefix and JSON plan header precede the unchanged raw payload. Intermediate stages update the header and relay payload bytes; endpoints parse the payload with DuckDB.
- **Stdin handoff**: Parse headers only through the unbuffered duplicated descriptor in `src/stream.rs`. Exact reads prevent Rust from retaining bytes before DuckDB opens `/dev/stdin`.
- **Source semantics**: File inputs remain canonicalized path references. Every non-path input is a single-pass stream; there is no materialized fallback.
- **Format passthrough**: Non-preset strings passed directly as DuckDB expressions (read) or COPY options (write) — no validation
- **DuckDB connection**: In-memory per invocation
- **Test fixtures are TOML**: Each file defines `cmd` (pipeline template with `{dq}` placeholder) + `[[cases]]` array
- **Test generation**: `fixture_tests_dir!` macro auto-discovers all `.toml` in dir, generates one `#[test]` per case
- **Expect variants**: `same` (stdout == input), `exact` (stdout == expected), `stderr_contains` (failure case)
- **Output normalization**: Tests trim whitespace, normalize CRLF → LF before comparison
- **Shell execution**: Tests run pipelines via `bash -o pipefail -c` — real process spawning, not in-process

## ANTI-PATTERNS

- **Do NOT** add in-process test helpers that bypass the CLI binary — tests must exercise the real pipeline
- **Do NOT** read a framed pipeline through `StdinLock`, `BufReader`, `read_to_string`, or `serde_json::from_reader`; buffered read-ahead loses payload bytes at the DuckDB handoff
- **Do NOT** use `success = false` with `expect.kind = "same"` — validated at compile time
- **Do NOT** duplicate case names within a fixture file — proc-macro rejects at compile time
- **Do NOT** add unknown fields to fixture TOML — `deny_unknown_fields` enforced

## COMMANDS

```bash
cargo build --release     # Build binary → target/release/dq
cargo test                # Run all tests (compiles fixtures → test fns)
cargo run -- from json   # Dev-run a single stage
make check                # Final verification after editing Rust code
```

After editing Rust code, always run `make check` as the final verification step.

## NOTES

- `select`/`where` args are interpolated into SQL strings — no sanitization, assumes trusted input
- Empty stdin behavior varies by DuckDB reader and command; assert observed endpoint behavior in fixtures rather than assuming a shared result
- Rust edition 2024
