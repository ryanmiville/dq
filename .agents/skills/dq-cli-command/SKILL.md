---
name: dq-cli-command
description: Change dq CLI subcommands, query-plan operations, or pipeline transport; add command fixtures.
---

# dq CLI commands

## Workflow

1. **Classify the change.** Read `AGENTS.md` for repository conventions and choose an existing command with the same role: source, transform, sink, inspection, or utility. For coverage-only work, choose the nearest fixture. Format recipes and URL-scheme setup belong in [dq-custom-plugin](../dq-custom-plugin/SKILL.md). Done when the role, analogue, and transport impact are identified.

2. **Trace the vertical slice.** Start at the command enum and dispatch in `src/main.rs`, its handler, and the nearest TOML fixture. Follow the branches the change reaches:
   - Query transforms: read [the transform pattern](references/command-pattern.md) and `src/plan.rs`.
   - Source/destination resolution: read `src/format.rs` and the registry in `src/plugins.rs`.
   - Plugin installation or configuration: read the corresponding functions in `src/plugins.rs` and `tests/test_cases/install.toml` or the nearest `plugin_*.toml` fixture.
   - Framing or payload lifecycle: read `src/stream.rs` and the existing `tests/stream_transport.rs` checks.

   Use the implementation for current types and signatures. Done when every affected dispatch, plan, resolution, and execution boundary has an identified change or an explicit reason to remain as-is.

3. **Implement through the shared paths.** Reuse the analogue's handler and transport helpers. Intermediate stages carry resolved source SQL and setup recipes with the plan; the executing endpoint opens DuckDB and prepares source and destination plugins. Keep header reads unbuffered and forward the payload byte-for-byte. Preserve file sources as references and stdin as a single-pass stream. Done when the changed command works both in a pipe and, where applicable, through terminal auto-display.

4. **Verify observable behavior.** Follow [the fixture workflow](references/testing-pattern.md). Cover execution and compiled SQL for a query change; cover the affected source, destination, or utility behavior for other roles. Run the focused fixtures, existing transport checks when transport changed, and `make check` after Rust changes. Done when the relevant checks pass, or a concrete validation blocker is reported.
