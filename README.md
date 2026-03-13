# dq

`dq` is a small CLI for data pipelines powered by DuckDB.

It lets you compose commands like `from`, `where`, `select`, `order-by`, `describe`, `summarize`, and `to` in Unix pipes:

```bash
echo '{"name":"Ada","age":37}' |
  dq from json |
  dq where "age >= 30" |
  dq select "name" |
  dq to json
```

## What it is

`dq` is designed for lightweight, shell-first querying and format conversion.

- Parse input into a queryable table (`from`)
- Apply SQL-style transforms (`where`, `select`, `order-by`)
- Inspect inferred schemas (`describe`)
- Compute per-column summary statistics (`summarize`)
- Emit output in a target format (`to`)

Built with:
- [DuckDB](https://duckdb.org/) for execution
- [clap](https://github.com/clap-rs/clap) for CLI parsing

## Current command set

- `dq from <format-or-path>`
- `dq to <format-or-path>`
- `dq select <columns>`
- `dq where <clause>`
- `dq limit <count>`
- `dq order-by <clause>`
- `dq describe`
- `dq summarize`

### Preset formats

`from` supports these presets:

- `csv`
- `json`
- `json-array`

`to` supports these presets:

- `csv`
- `json`
- `json-array`
- `pretty`

`from` and `to` also accept file paths directly, so you can point at files without wrapping them in SQL quotes.

Examples:

```bash
# read a file directly
dq from ../testdata.json

# write a file directly
dq to ../out/result.csv

# raw DuckDB read expression
dq from --expr "read_csv('/dev/stdin')"

# raw DuckDB COPY options
dq to --expr "(FORMAT CSV, DELIMITER '|', HEADER)"
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
cargo run -- from json
```

## Quick examples

### JSON filter + projection

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' |
  dq from json |
  dq where "age >= 40" |
  dq select "name" |
  dq to json
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

### Pretty table output

```bash
printf '[{"name":"Ada","age":37},{"name":"Linus","age":54}]\n' |
  dq from json |
  dq to pretty
```

### File-to-file conversion

```bash
dq from ../data/input.json |
  dq where "age >= 40" |
  dq to ../data/filtered.json
```

### Terminal auto-display

When `dq from`, `dq select`, or `dq where` write directly to a terminal, they display a pretty table automatically. When their output is piped, they continue emitting Arrow for the next `dq` stage.

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' |
  dq from json |
  dq where "age >= 40"
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
