# Testing pattern

Use TOML fixtures in `tests/test_cases/`. Do not add bespoke in-process tests for CLI behavior.

## Fixture shapes

### End-to-end transform fixture

Start from `tests/test_cases/select.toml`, `where_clause.toml`, `limit.toml`, or `order_by.toml`.

```toml
cmd = "{dq} from json | {dq} <command> \"...\" | {dq} to json"

[[cases]]
name = "example"
input = """
{"name":"Ada","age":37}
"""
[cases.expect]
kind = "exact"
stdout = """
...
"""
```

### Direct plan-input fixture

Useful for sink or compiler behavior.

```toml
cmd = "{dq} to json"

[[cases]]
name = "example"
input = """
{"version":1,"source_path":"tests/data/people.json","ops":[...]}
"""
[cases.expect]
kind = "exact"
stdout = """
...
"""
```

## Common cases

For a transform command, usually include:

- nominal single-row or small multi-row success
- edge case with zero matching rows or changed shape, if applicable
- empty stdin case with the actual lazy-pipeline behavior

Do not assume the old Arrow-era `IO Error`. Under the plan pipeline, empty `from json` input often yields a `json` column of type `JSON`, so later commands may fail with binder errors instead.

## Expectations

- Use `kind = "exact"` for expected output text
- Use `kind = "same"` only when output should equal input exactly
- Use `kind = "stderr_contains"` for failure cases

## Harness behavior to remember

- Runner executes `bash -o pipefail -c`
- Output is normalized for whitespace and CRLF
- `{dq}` expands to the built test binary
- Unknown fields and duplicate case names are rejected by fixture parsing/codegen

## Validation flow

1. Run the most targeted fixture test you can
2. If fixture parsing fails, inspect compile-time errors first
3. If command output differs, compare normalized expected vs actual output
4. Finish with `cargo test`
