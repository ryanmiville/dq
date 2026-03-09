# PROJECT KNOWLEDGE BASE

**Generated:** 2026-03-08
**Commit:** 83c55bb
**Branch:** main

## OVERVIEW

`dq` — small Rust CLI for shell-first data pipelines powered by DuckDB. Pipe-composes `from`, `to`, `select`, `where` subcommands; stages exchange Arrow over stdin/stdout.

## STRUCTURE

```
dq/
├── src/
│   ├── main.rs         # CLI entry, clap parsing, DuckDB connection setup
│   ├── cmd.rs          # Subcommand implementations (from/to/select/where)
│   └── format.rs       # Format enum: presets (csv/json/jsonl) + raw passthrough
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
- **Pipeline protocol**: All inter-stage data is Arrow format via `/dev/stdin` → `/dev/stdout`. Only `from` reads raw formats; only `to` writes raw formats.
- **Format passthrough**: Non-preset strings passed directly as DuckDB expressions (read) or COPY options (write) — no validation
- **DuckDB connection**: In-memory, installs `arrow` extension from community repo on every invocation
- **Test fixtures are TOML**: Each file defines `cmd` (pipeline template with `{dq}` placeholder) + `[[cases]]` array
- **Test generation**: `fixture_tests_dir!` macro auto-discovers all `.toml` in dir, generates one `#[test]` per case
- **Expect variants**: `same` (stdout == input), `exact` (stdout == expected), `stderr_contains` (failure case)
- **Output normalization**: Tests trim whitespace, normalize CRLF → LF before comparison
- **Shell execution**: Tests run pipelines via `bash -o pipefail -c` — real process spawning, not in-process

## ANTI-PATTERNS

- **Do NOT** add in-process test helpers that bypass the CLI binary — tests must exercise the real pipeline
- **Do NOT** use `success = false` with `expect.kind = "same"` — validated at compile time
- **Do NOT** duplicate case names within a fixture file — proc-macro rejects at compile time
- **Do NOT** add unknown fields to fixture TOML — `deny_unknown_fields` enforced

## COMMANDS

```bash
cargo build --release     # Build binary → target/release/dq
cargo test                # Run all tests (compiles fixtures → test fns)
cargo run -- from jsonl   # Dev-run a single stage
```

## NOTES

- `select`/`where` args are interpolated into SQL strings — no sanitization, assumes trusted input
- Empty stdin behavior varies by command: `from csv` returns header-only, `from json` returns `[]`, `select`/`where` on empty Arrow fails with IO Error
- Rust edition 2024
