---
name: dq-cli-command
description: Add or modify dq CLI subcommands in this repo. Use when implementing a new pipeline stage, wiring clap command parsing, following the plan-appending command pattern, or adding TOML fixture coverage for command behavior.
references:
  - references/command-pattern.md
  - references/testing-pattern.md
---

# dq-cli-command

## Overview

Add new `dq` subcommands by following the lightweight plan-based pattern used by `select`, `where`, `limit`, `order-by`, `describe`, and `summarize`: define clap surface in `src/main.rs`, append to the query `Plan` in `src/cmd.rs`, update `src/plan.rs` if a new op is needed, and add fixture coverage in `tests/test_cases/`.

Prefer this skill for command-shaped pipeline stages that consume JSON plan input from stdin and either emit an updated plan or pretty-print the compiled query when stdout is a TTY.

## Use this skill when

- Add a new CLI subcommand to `dq`
- Refactor an existing subcommand to match the plan-appending command pattern
- Extend command tests using the fixture harness
- Need the repo-specific checklist for command wiring, plan compilation, and validation

## Workflow

1. Read `AGENTS.md`, `src/main.rs`, `src/cmd.rs`, `src/plan.rs`, `tests/common/mod.rs`, and the nearest fixture files in `tests/test_cases/`.
2. Load `references/command-pattern.md` to mirror the implementation shape used by current transform commands.
3. Load `references/testing-pattern.md` to add fixture coverage with the existing TOML schema.
4. Implement the command with the smallest possible surface area:
   - add one `Command` enum variant in `src/main.rs`
   - add or update one `Op` variant plus compiler lowering in `src/plan.rs` if the command changes query semantics
   - add one handler function in `src/cmd.rs`
   - dispatch to that handler from `run()`
5. Preserve pipeline conventions:
   - transform stages read `Plan` from stdin with `Plan::read_from(io::stdin().lock())`
   - append one op with `plan.with_op(Op::...)`
   - route through `emit_plan_or_pretty()`
   - sink stages execute `COPY ({plan.compile_sql()}) TO ...`
   - source stages either seed a `Plan` from a file path or materialize non-path input to temp parquet first
6. Add or update one fixture file in `tests/test_cases/`.
7. Run targeted validation first, then broader validation:
   - `cargo test <targeted test name>` if convenient
   - `cargo test`

## Rules specific to dq

- Keep new command handlers adjacent to the existing plan-appending handlers in `src/cmd.rs`.
- Match the naming convention already in use: concise clap command name, descriptive Rust fn name if needed.
- Reuse `emit_plan_or_pretty()` for transform-like commands instead of duplicating tty/plan branching.
- Keep compiler output relation-shaped. `Plan::compile_sql()` must stay valid inside `COPY ({compiled}) TO ...`.
- Preserve ordered semantics. New ops should compose by nesting, not by flattening prior steps unsafely.
- Keep SQL interpolation simple and explicit; this project assumes trusted input.
- Do not bypass the CLI in tests; use TOML fixtures and the shell runner only.
- Mirror empty-stdin behavior intentionally under the new plan pipeline. Do not assume the old Arrow-era `IO Error`; assert the actual observed behavior.

## Output expectations

When finishing work, report:

- files changed
- command shape added
- plan/compiler changes, if any
- test fixture added or updated
- validation run and result
