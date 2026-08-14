---
name: dq-cli-command
description: Add or change dq CLI commands and their pipeline tests. Use for source, transform, sink, or inspection subcommands; clap wiring; plan operations or SQL lowering; and TOML command fixtures.
---

# dq CLI commands

## Workflow

1. **Classify** the task and choose its closest current analogue after reading `AGENTS.md`:
   - **source** — creates a path-backed plan or a single-pass stream plan
   - **transform** — appends an `Op` and relays the framed plan plus unchanged payload
   - **sink** — executes a plan and consumes or hands off its payload
   - **inspection** — reports on a plan without necessarily executing it; for example, `sql` drains the payload so upstream can finish
   - **coverage-only** — changes command expectations without production wiring

   Complete this step when the role, analogue, and any transport impact are explicit.

2. **Trace the vertical slice** through the current code. For production changes, inspect the command enum and dispatch in `src/main.rs`, the analogous handler in `src/cmd.rs`, and its nearest fixture. For coverage-only changes, start at the nearest fixture. Inspect only the branch-specific components:
   - transforms: `src/plan.rs` and [the transform pattern](references/command-pattern.md)
   - `from`/`to` formats or presets: `src/format.rs`
   - framing, payload forwarding, source handoff, or endpoint draining: `src/stream.rs` and `tests/stream_transport.rs`
   - fixture schema or harness behavior: `dq_test_fixtures/src/lib.rs`, `dq_test_macros/src/lib.rs`, and `tests/common/mod.rs`

   Complete this step when every integration point required by the role is accounted for.

3. **Implement the vertical slice.** Reuse the shared command and stream paths shown by the current analogue. Keep framed header reads unbuffered, preserve stream payload bytes exactly, and retain path sources as canonicalized user-owned references. Compile plan operations in order as nested, relation-shaped SQL that remains valid in nested queries and `CREATE TEMP TABLE ... AS`.

4. **Cover every changed behavior.** Follow [the testing pattern](references/testing-pattern.md). Exercise user-visible command behavior through the real CLI fixture harness; use unit tests for plan lowering or serialization invariants and transport tests for binary framing or payload lifecycle behavior.

5. **Validate to completion.** Run the narrowest relevant test first, then run `make check`. Completion requires both to pass.

## Finish

Report the command behavior, files changed, coverage added, and validation result. Treat the current code and `AGENTS.md` as the source of truth if a reference conflicts with them.
