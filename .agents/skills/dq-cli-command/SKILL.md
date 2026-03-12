---
name: dq-cli-command
description: Add or modify dq CLI subcommands in this repo. Use when implementing a new pipeline stage, wiring clap command parsing, following the select/where command pattern, or adding TOML fixture coverage for command behavior.
references:
  - references/command-pattern.md
  - references/testing-pattern.md
---

# dq-cli-command

## Overview

Add new `dq` subcommands by following the lightweight pattern used by `select` and `where`: define clap surface in `src/main.rs`, implement a small SQL-building handler in `src/cmd.rs`, and add fixture coverage in `tests/test_cases/`.

Prefer this skill for command-shaped pipeline stages that consume Arrow from stdin and emit Arrow or pretty output through the shared helpers in `src/cmd.rs`.

## Use this skill when

- Add a new CLI subcommand to `dq`
- Refactor an existing subcommand to match `select` / `where`
- Extend command tests using the fixture harness
- Need the repo-specific checklist for command wiring and validation

## Workflow

1. Read `AGENTS.md`, `src/main.rs`, `src/cmd.rs`, `tests/common/mod.rs`, and the nearest fixture files in `tests/test_cases/`.
2. Load `references/command-pattern.md` to mirror the implementation shape used by `select` and `where`.
3. Load `references/testing-pattern.md` to add fixture coverage with the existing TOML schema.
4. Implement the command with the smallest possible surface area:
   - add one `Command` enum variant in `src/main.rs`
   - add one handler function in `src/cmd.rs`
   - dispatch to that handler from `run()`
5. Preserve pipeline conventions:
   - read Arrow from `/dev/stdin` for transform stages
   - build a SQL query string, then route through `emit_relation_query()`
   - attach command-specific context with `anyhow::Context`
6. Add or update one fixture file in `tests/test_cases/`.
7. Run targeted validation first, then broader validation:
   - `cargo test <targeted test name>` if convenient
   - `cargo test`

## Rules specific to dq

- Keep new command handlers adjacent to `select()` and `where_clause()` in `src/cmd.rs`.
- Match the naming convention already in use: concise command name in clap, descriptive Rust fn name if needed.
- Reuse `emit_relation_query()` for transform-like commands instead of duplicating Arrow/pretty branching.
- Keep SQL interpolation simple and explicit; this project assumes trusted input.
- Do not bypass the CLI in tests; use TOML fixtures and the shell runner only.
- Mirror empty-stdin behavior intentionally; if the new command reads Arrow like `select`/`where`, an empty input often fails with `IO Error` and should usually be tested.

## Output expectations

When finishing work, report:

- files changed
- command shape added
- test fixture added or updated
- validation run and result
