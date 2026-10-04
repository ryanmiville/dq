# dq

Shell-first data pipelines powered by DuckDB.

Compose `from`, `select`, `where`, `order-by`, `limit`, `describe`, `summarize`, and `to` in Unix pipes. Intermediate stages exchange a private framed query plan followed by the original raw input stream. `dq to ...` executes the accumulated plan and writes results, while `dq sql` prints the compiled query without executing it.

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

Run `dq --skill` to print the self-contained custom plugin authoring skill as Markdown. The skill is embedded in the binary and matches its plugin API.

- `dq from <format-or-path>`
- `dq to <format-or-path>`
- `dq sql`
- `dq select <columns>`
- `dq where <clause>`
- `dq limit <count>`
- `dq offset <count>`
- `dq order-by <clause>`
- `dq describe`
- `dq summarize`
- `dq install <url> [--replace]`

### Preset formats

`from` and `to` bundle these presets:

- `csv`
- `json`
- `json-array`

`to` also supports `pretty`. YAML is available as an [optional plugin](plugins/optional/README.md).

`from` and `to` also accept file paths and URLs directly, so you can point at files without wrapping them in SQL quotes. The bundled S3 storage plugin supports public or authenticated S3 reads and writes. Quote glob patterns, such as `dq from 'data/*.parquet'`, to let DuckDB expand them.

### Plugins and global configuration

Bundled formats and S3 support are TOML plugins embedded in the binary at build time. Adding a file matching `plugins/bundled/*.toml` includes it automatically in subsequent builds. Plugins in `plugins/optional/` are available for manual installation and are not embedded or loaded automatically. User plugins use the same schema and load without rebuilding dq.

For example, install the optional YAML plugin:

```bash
dq install https://github.com/ryanmiville/dq/blob/main/plugins/optional/yaml.toml
```

YAML stdin currently fails in the DuckDB YAML extension; use `.yaml` or `.yml` file paths for input. After installation, `dq to yaml` and `.yaml`/`.yml` destinations are supported. See the [optional plugin catalog](plugins/optional/README.md) for installation commands and limitations.

Install a user plugin from a URL pointing directly to its TOML file:

```bash
dq install https://github.com/someone/dq-formats/blob/main/plugins/rows.toml
dq install --replace https://github.com/someone/dq-formats/blob/v1.2.0/plugins/rows.toml
```

GitHub file URLs are converted to raw download URLs. Other HTTP/HTTPS URLs, including raw GitHub URLs, work directly. The URL selects the branch, tag, or commit. dq validates the downloaded plugin and registration conflicts before saving it as `<plugin-id>.toml` in the global plugin directory. Installed IDs use letters, digits, `.`, `-`, or `_`, and cannot be `.` or `..`.

An existing user plugin requires `--replace`. Replacement is atomic; a failed download or validation leaves the previous file intact. To replace a bundled plugin, disable it in configuration first. Installation saves the recipes; DuckDB extensions install and setup SQL executes when the plugin is used.

User configuration lives at `$XDG_CONFIG_HOME/dq`, or `~/.config/dq` when `XDG_CONFIG_HOME` is unset. The optional `config.toml` controls bundled plugins and additional plugin directories:

```toml
disabled_bundled = ["csv"]
plugin_dirs = ["extra", "/path/to/shared/plugins"]
```

dq always discovers `plugins/*.toml` inside its configuration directory. Relative entries in `plugin_dirs` resolve against that directory. Disabling uses the bundled plugin's `id`, so disabling `json` removes both `json` and `json-array`. User plugins can reuse a disabled bundled plugin's ID. Duplicate IDs, format names, suffixes, or URL schemes produce errors; load order never chooses a winner.

For example, save this as `~/.config/dq/plugins/csv.toml` alongside the configuration above to replace CSV with a pipe-delimited variant:

```toml
api_version = 1
id = "csv"
kind = "format"

[[formats]]
name = "csv"
suffixes = ["csv"]
read = "read_csv({{path}}, delim='|')"

[formats.write]
kind = "copy"
options = "FORMAT CSV, DELIMITER '|', HEADER"
```

`name` selects a format for stdin/stdout, while `suffixes` selects it for files and URLs. Names and suffixes match without regard to case. One plugin can declare multiple `[[formats]]` variants. A format can provide `read`, `write`, or both. `{{path}}` becomes a quoted SQL string containing the source or destination, including `/dev/stdin` or `/dev/stdout` for named formats. Unclaimed suffixes retain DuckDB's automatic format inference; disabling a plugin removes its registrations and setup, rather than preventing DuckDB from reading the format.

The `duckbox` writer selects dq's native table renderer instead of DuckDB `COPY`:

```toml
api_version = 1
id = "table"
kind = "format"

[[formats]]
name = "table"

[formats.write]
kind = "duckbox"
```

The bundled `pretty` plugin uses this writer and supplies terminal auto-display. A replacement `pretty` format can change terminal output; disabling it without a replacement makes terminal auto-display unavailable.

Plugins declare ordered setup steps for DuckDB extensions and SQL. For example, a YAML format plugin includes:

```toml
[[setup]]
kind = "extension"
name = "yaml"
repository = "community"
```

An extension step loads the extension, automatically installing it when needed. Omit `repository` for DuckDB's default repository. SQL steps can use environment values at execution time:

```toml
[[setup]]
kind = "sql"
requires_env = ["DQ_CA_CERT_FILE"]
sql = "SET ca_cert_file = {{env:DQ_CA_CERT_FILE}};"
```

`requires_env` skips a step when any listed variable is absent. Environment substitutions are supported in setup SQL and become quoted SQL strings. A storage plugin uses `kind = "storage"` and `schemes = ["s3"]` instead of format recipes. Storage and format plugins compose for each endpoint.

Source reader SQL and setup recipes travel with the plan, so downstream stages do not need to look up the source plugin again. Shared setup runs once per plugin per executing DuckDB connection, with source plugins preceding destination plugins. If setup SQL uses `{{path}}`, it refers to the first endpoint using that plugin. Environment values are read by the executing process. `dq sql` prints the reader query without running setup, installing extensions, or reading data.

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

### Read from S3

Use an `s3://` URI anywhere you would use a local input path. Public objects work without AWS credentials:

```bash
dq from s3://noaa-ghcn-pds/csv/by_year/1763.csv |
  dq limit 5 |
  dq to json
```

For private objects, make credentials available through the AWS credential chain:

```bash
export AWS_PROFILE=production
dq from s3://my-bucket/path/data.parquet |
  dq where "created_at >= DATE '2026-01-01'" |
  dq to json
```

DQ uses DuckDB's AWS credential chain, so credentials from environment variables, AWS config profiles, SSO sessions, web identity, and instance metadata are supported; for SSO, run `aws sso login --profile <profile>` first. When the chain finds credentials, DuckDB signs S3 requests with them; when it does not, DuckDB makes anonymous requests suitable for public objects.

For S3 connections that require a custom certificate authority, set `DQ_CA_CERT_FILE` to the CA bundle path. For example, environments that already provide an AWS-specific bundle can opt into using it with DQ:

```bash
export DQ_CA_CERT_FILE="$AWS_CA_BUNDLE"
```

On the first executed S3 query, DQ installs DuckDB's `httpfs` and `aws` extensions in DuckDB's user extension directory; later invocations reuse those installed files while loading them into each new in-memory connection.

The S3 plugin creates a temporary credential-chain secret once per connection, even when both the source and destination use S3. The secret lasts only for that connection and does not persist credentials to disk.

Because the final pipeline process executes the query, export AWS settings for the whole pipeline instead of assigning them only to `dq from`; for example, use `export AWS_PROFILE=production` rather than `AWS_PROFILE=production dq from ... | dq to ...`.

### Pretty table output

Use `to pretty` to request the same Duckbox table used for terminal auto-display, including when stdout is redirected:

```bash
cat data/input.json |
  dq from json |
  dq limit 5 |
  dq to pretty > preview.txt
```

Pretty output is display-oriented and may be limited by `DQ_MAX_ROWS`; use `csv` or `json` for lossless export.

### Inspect compiled SQL

End a pipeline with `dq sql` to print its compiled query without executing it:

```bash
dq from data/input.json |
  dq where "age >= 40" |
  dq select "name" |
  dq sql
```

```sql
SELECT name FROM (SELECT * FROM (SELECT * FROM read_json_auto('/absolute/path/data/input.json')) AS q WHERE age >= 40) AS q;
```

This prints the relation query represented by the plan, not the temporary-table and `COPY` statements used by `dq to`. Queries for streamed input reference `/dev/stdin` and require the original data on stdin if executed separately.

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
- Local file and S3 inputs remain deferred references in the query plan; S3 extensions and credentials are initialized only by the process that executes the plan.
- `dq sql` prints S3 plans without installing extensions, loading credentials, or accessing the network.
- `select`/`where` args are interpolated into SQL — keep inputs trusted.
