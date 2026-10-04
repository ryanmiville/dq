# Command testing pattern

## Coverage boundary

Add coverage through TOML fixtures in `tests/test_cases/`. The harness launches the real binary through `bash -o pipefail -c`; plan lowering, operation order, and plugin setup are observable through execution or a `dq sql` endpoint. Retain existing unit and transport tests in validation.

If a material invariant cannot be exercised through fixtures, explain the gap and ask before adding a unit test or a new test outside the fixture harness.

## Fixture workflow

1. Copy the nearest fixture shape. Read `dq_test_fixtures/src/lib.rs` for accepted fields and expectations; read `tests/common/mod.rs` when relying on normalization or process behavior.
2. Cover a result that changes if the behavior regresses. For transforms, include ordered composition and a `dq sql` case. Derive empty-input expectations from the actual reader rather than assuming all readers behave alike. Done when each changed branch has a result or failure assertion.
3. For plugins, isolate configuration under a temporary `XDG_CONFIG_HOME`. The harness already supplies an isolated directory; follow a `plugin_*.toml` analogue when a fixture needs its own files. Use cleanup traps for additional temporary directories. Copy the real manifest into fixtures for repository plugins so tests exercise the shipped recipes.
4. When adding a fixture file, run `touch tests/fixtures.rs` so the directory-enumerating proc macro discovers it in incremental builds. Confirm the intended test appears in the focused run.

The harness normalizes CRLF and per-line indentation/trailing whitespace; it does not perform arbitrary whitespace normalization.

## Validation

Generated fixture test names combine the fixture stem and case name. Run the narrowest matching case first; for example:

```bash
cargo test --test fixtures plugin_shared_setup_once_across_variants_and_stages
```

Run `make check` for Rust or repository plugin changes. For documentation-only edits, check links and execute changed examples without adding test files.
