# Transform command pattern

Use this branch for commands that append one semantic operation while preserving the upstream stream.

## Vertical slice

1. Add the clap variant and argument documentation in `src/main.rs`, then dispatch it to a thin command-specific handler.
2. Add an `Op` variant in `src/plan.rs` when the command changes query semantics.
3. Lower the op in `compile_op()` over the supplied `input` relation. Nest prior SQL so operation order remains observable.
4. Keep lowering relation-shaped. Metadata operations such as `DESCRIBE` and `SUMMARIZE` must produce selectable SQL that also works under `CREATE TEMP TABLE ... AS`.
5. In `src/cmd.rs`, construct the op and delegate to the shared `transform(conn, op, context)` path. That path owns framed header reads, byte-exact payload forwarding, TTY execution and pretty rendering, and broken-pipe handling.
6. Add CLI fixture coverage and, when lowering or composition is non-trivial, a focused compiler test in `src/plan.rs`.

Complete the transform only when clap help, dispatch, handler, plan serialization/lowering, ordered composition, and externally visible behavior are all accounted for.

## Transport invariant

A non-TTY transform rewrites only the framed plan header and forwards the remaining payload unchanged. A TTY transform hands a stream payload to DuckDB, executes the compiled plan, and renders the result. Keep custom command code above this boundary; extend the shared transport path only when the command genuinely changes transport behavior.
