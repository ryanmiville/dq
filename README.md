# dq

Shell-first data pipelines powered by DuckDB.

Compose `from`, `select`, `where`, `order-by`, `limit`, `describe`, `summarize`, and `to` in Unix pipes. Intermediate stages exchange a private framed query plan followed by the original raw input stream. `dq to ...` executes the accumulated plan and writes results.

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' |
  dq from json |
  dq where "age >= 40" |
  dq select "name" |
  dq to json
# {"name":"Linus"}
```

When stdout is a terminal, stages auto-execute the accumulated plan and pretty-print a table instead of emitting the internal stream:

```
$ printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' | dq from json
┌─────────┬────────┐
│  name   │  age   │
│ varchar │ bigint │
├─────────┼────────┤
│ Ada     │     37 │
│ Linus   │     54 │
└─────────┴────────┘
```

## Install

```bash
brew install ryanmiville/tap/dq
```

Or build from source (requires Rust stable):

```bash
cargo build --release
```

## Current command set

- `dq from <format-or-path>`
- `dq to <format-or-path>`
- `dq select <columns>`
- `dq where <clause>`
- `dq limit <count>`
- `dq offset <count>`
- `dq order-by <clause>`
- `dq describe`
- `dq summarize`

### Preset formats

`from` and `to` support these presets:

- `csv`
- `json`
- `json-array`

`from` and `to` also accept file paths directly, so you can point at files without wrapping them in SQL quotes.

## Examples

### Filter and project

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' |
  dq from json |
  dq where "age >= 40" |
  dq select "name" |
  dq to json
```

```
{"name":"Linus"}
```

### Select with expressions

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' |
  dq from json |
  dq select "name, age * 2 AS double_age" |
  dq to json
```

```
{"name":"Ada","double_age":74}
{"name":"Linus","double_age":108}
```

### Order by

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n{"name":"Grace","age":85}\n' |
  dq from json |
  dq order-by "age DESC" |
  dq to json
```

```
{"name":"Grace","age":85}
{"name":"Linus","age":54}
{"name":"Ada","age":37}
```

### Limit

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n{"name":"Grace","age":85}\n' |
  dq from json |
  dq limit 1 |
  dq to json
```

```
{"name":"Ada","age":37}
```

### Offset

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n{"name":"Grace","age":85}\n' |
  dq from json |
  dq offset 1 |
  dq limit 1 |
  dq to json
```

```
{"name":"Linus","age":54}
```

### Format conversion

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' |
  dq from json |
  dq to csv
```

```
name,age
Ada,37
Linus,54
```

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' |
  dq from json |
  dq to json-array
```

```json
[
	{"name":"Ada","age":37},
	{"name":"Linus","age":54}
]
```

### Describe

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' |
  dq from json |
  dq describe
```

```
┌─────────────┬─────────────┬─────────┬─────────┬─────────┬─────────┐
│ column_name │ column_type │  null   │   key   │ default │  extra  │
│   varchar   │   varchar   │ varchar │ varchar │ varchar │ varchar │
├─────────────┼─────────────┼─────────┼─────────┼─────────┼─────────┤
│ name        │ VARCHAR     │ YES     │ NULL    │ NULL    │ NULL    │
│ age         │ BIGINT      │ YES     │ NULL    │ NULL    │ NULL    │
└─────────────┴─────────────┴─────────┴─────────┴─────────┴─────────┘
```

### Summarize

```bash
printf '{"name":"Ada","age":37}\n{"name":"Linus","age":54}\n' |
  dq from json |
  dq summarize
```

```
┌─────────────┬─────────────┬─────────┬─────────┬───────────────┬─────────┬────────────────────┬─────────┬─────────┬─────────┬────────┬─────────────────┐
│ column_name │ column_type │   min   │   max   │ approx_unique │   avg   │        std         │   q25   │   q50   │   q75   │ count  │ null_percentage │
│   varchar   │   varchar   │ varchar │ varchar │    bigint     │ varchar │      varchar       │ varchar │ varchar │ varchar │ bigint │  decimal(9,2)   │
├─────────────┼─────────────┼─────────┼─────────┼───────────────┼─────────┼────────────────────┼─────────┼─────────┼─────────┼────────┼─────────────────┤
│ name        │ VARCHAR     │ Ada     │ Linus   │             2 │ NULL    │ NULL               │ NULL    │ NULL    │ NULL    │      2 │            0.00 │
│ age         │ BIGINT      │ 37      │ 54      │             2 │ 45.5    │ 12.020815280171307 │ 37      │ 46      │ 54      │      2 │            0.00 │
└─────────────┴─────────────┴─────────┴─────────┴───────────────┴─────────┴────────────────────┴─────────┴─────────┴─────────┴────────┴─────────────────┘
```

### File I/O

```bash
# read from file
dq from data/input.json | dq where "age >= 40" | dq to csv

# write to file
dq from data/input.json | dq to data/filtered.csv
```

### Pretty table output

Use `to pretty` to request the same Duckbox table used for terminal auto-display, including when stdout is redirected:

```bash
cat data/input.json |
  dq from json |
  dq limit 5 |
  dq to pretty > preview.txt
```

Pretty output is display-oriented and may be limited by `DQ_MAX_ROWS`; use `csv` or `json` for lossless export.

### Raw DuckDB expressions

Use `--expr` to pass arbitrary DuckDB read/copy expressions:

```bash
# custom read
printf 'name,age\nAda,37\n' |
  dq from --expr "read_csv('/dev/stdin', header=true)" |
  dq to json
```

```
{"name":"Ada","age":37}
```

```bash
# custom write (pipe-delimited)
printf 'name,age\nAda,37\n' |
  dq from csv |
  dq to --expr "'/dev/stdout' (FORMAT CSV, DELIMITER '|', HEADER)"
```

```
name|age
Ada|37
```

## Terminal auto-display

When a stage's stdout is a terminal, it executes the accumulated plan and renders a native DuckDB duckbox table. When piped or redirected, it emits a private framed plan and relays any raw input payload for the next `dq` stage. This means the last command in a pipeline automatically pretty-prints without needing `dq to`, but if you want materialized rows in a pipe or file you should end the pipeline with `dq to ...`.

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DQ_MAX_WIDTH` | min(stdout TTY width, `120`) | Max table width in cells; `0` = default |
| `DQ_MAX_ROWS` | `20` | Max rows before truncation; `0` = unlimited |
| `NO_COLOR` | — | Disable ANSI color when set |

These settings apply to both terminal auto-display and `dq to pretty`.

## Notes

- Intermediate pipeline stages exchange a private framed plan and stream non-path input bytes unchanged.
- `dq to ...` executes the full accumulated plan in DuckDB and parses streamed input at the endpoint.
- File inputs remain path references; `dq` does not copy or delete user-owned source files.
- `select`/`where` args are interpolated into SQL — keep inputs trusted.
