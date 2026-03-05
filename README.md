# dq

`dq` is a small CLI for data pipelines powered by DuckDB.

It lets you compose commands like `from`, `where`, `select`, and `to` in Unix pipes:

```bash
echo '{"name":"Ada","age":37}' |
  dq from jsonl |
  dq where "age >= 30" |
  dq select "name" |
  dq to jsonl
```

## What it is

`dq` is designed for lightweight, shell-first querying and format conversion.

- Parse input into a queryable table (`from`)
- Apply SQL-style transforms (`where`, `select`)
- Emit output in a target format (`to`)

Built with:
- [DuckDB](https://duckdb.org/) for execution
- [clap](https://github.com/clap-rs/clap) for CLI parsing

## Current command set

- `dq from <format-or-expression>`
- `dq to <format-or-copy-options>`
- `dq select <columns>`
- `dq where <clause>`

### Preset formats

`from`/`to` support these presets:

- `csv`
- `json`
- `jsonl`

You can also pass raw DuckDB expressions/options for advanced use.

Examples:

```bash
# custom read expression
dq from "read_csv('/dev/stdin')"

# custom COPY options
dq to "(FORMAT CSV, DELIMITER '|', HEADER)"
```

## Install / run

### Prerequisites

- Rust toolchain (stable)

### Build

```bash
cargo build --release
```

Binary path:

```bash
./target/release/dq
```

You can also run with Cargo during development:

```bash
cargo run -- from jsonl
```

## Quick examples

### JSONL filter + projection

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' |
  dq from jsonl |
  dq where "age >= 40" |
  dq select "name" |
  dq to jsonl
```

### CSV round-trip

```bash
printf 'name,age\nAda,37\nLinus,54\n' |
  dq from csv |
  dq to csv
```

### JSON array round-trip

```bash
printf '[{"name":"Ada","age":37}]\n' |
  dq from json |
  dq to json
```

## Development

Run tests:

```bash
cargo test
```

## Notes

- Commands are intended to be composed in a pipeline.
- `select` and `where` arguments are inserted into SQL expressions; keep inputs trusted.
- Under the hood, pipeline stages exchange Arrow data for efficient chaining.
