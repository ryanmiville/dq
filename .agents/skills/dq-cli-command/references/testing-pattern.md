# Command testing pattern

## Choose the test layer

- **CLI behavior:** add cases to the nearest TOML fixture in `tests/test_cases/`. The harness launches the real binary through `bash -o pipefail -c`; exercise command behavior through that real-process path.
- **Plan behavior:** add focused `src/plan.rs` unit tests for serialization, SQL lowering, nesting, or operation order.
- **Transport behavior:** add `src/stream.rs` unit tests or `tests/stream_transport.rs` integration tests for framing, binary payload preservation, endpoint handoff or draining, large streams, and broken pipes. Construct command input through upstream `dq` stages; use transport tests when manual binary frames are required.

Complete test selection when every changed externally visible branch and every new internal invariant has a layer.

## Fixture workflow

1. Copy the nearest fixture shape and keep its end-to-end pipeline through `from`, the command under test, and an endpoint such as `to` or `sql`.
2. Cover nominal behavior plus any edge or failure behavior introduced by the command. Derive empty-input expectations from the current pipeline and assert the observed result.
3. Use `kind = "exact"` for expected stdout, `kind = "same"` for normalized stdout-versus-input comparison, and `kind = "stderr_contains"` with `success = false` for an expected failure.
4. When adding a new fixture file, run `touch tests/fixtures.rs` so the directory-enumerating proc macro discovers it in incremental builds.

The harness normalizes CRLF and per-line indentation/trailing whitespace; it does not perform arbitrary whitespace normalization.

## Validation

Generated fixture test names combine the fixture stem and case name. Run the narrowest matching case first:

```bash
cargo test --test fixtures <fixture_stem>_<case_name>
```

Finish with `make check`.
