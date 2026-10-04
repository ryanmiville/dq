---
name: dq-custom-plugin
description: Author dq TOML format or storage plugins with read/write variants and DuckDB extension setup.
---

# dq custom plugins

Create a standalone TOML manifest that an installed dq binary can load. DuckDB supplies the readers, writers, extensions, and setup SQL. This guide is also available through `dq --skill` and matches that binary's plugin API.

## Workflow

1. **Choose the recipes.** Identify the requested read/write directions, names, suffixes, and DuckDB dependencies. Choose a format plugin for reader/writer recipes or a storage plugin for URL-scheme setup. Done when each requested direction has a DuckDB recipe and its extension requirements are known.
2. **Write the manifest.** Use the schema and examples below. Group related variants in one plugin, giving suffix claims to the appropriate variant. Express fixed options as named formats and leave unsupported directions absent. Done when the manifest fits the schema and its registrations are free of conflicts.
3. **Declare setup.** Add ordered steps for extensions, credentials, or connection SQL. Follow the setup lifecycle below. Done when dependencies are initialized before use, including when the plugin serves both endpoints.
4. **Validate in isolation.** Use a temporary configuration and representative data. Check each supported variant, named stdin/stdout, file suffix selection, and URL endpoints where applicable. Done when each advertised path has a passing check or a specific documented limitation.
5. **Distribute the TOML file.** Provide a full-file installation URL and an update command using `--replace`, or the local discovery path. Done when users can install and use the manifest without rebuilding dq.

## Manifest API

| Field | Meaning |
| --- | --- |
| `api_version` | Required; currently `1`. |
| `id` | Required plugin ID. For URL installation use letters, digits, `.`, `-`, or `_`; `.` and `..` alone are invalid. |
| `kind` | Required; `"format"` or `"storage"`. |
| `formats` | One or more `[[formats]]` entries for a format plugin. |
| `schemes` | One or more URL scheme names for a storage plugin, such as `["s3"]`. |
| `setup` | Optional ordered `[[setup]]` steps, available to either kind. |

Format plugins declare formats and no schemes; storage plugins declare schemes and no formats. Each format requires a `name` and at least one of `read` or `write`; `suffixes` is optional. Readers are DuckDB table expressions. Writers select `kind = "copy"` with an `options` string, or `kind = "duckbox"` for dq's native table renderer.

Names, suffixes, and schemes match without regard to case. Suffixes omit the leading dot and match the final file extension; URL queries and fragments are excluded. Duplicate IDs or claims fail rather than choosing a winner by load order. Unclaimed suffixes retain DuckDB's automatic inference. Executable converters and plugin-defined CLI flags require changes to dq itself.

## Format example

Save this as `rows.toml`. It reads pipe-delimited data and offers two output variants; only the default claims `.rows`:

```toml
api_version = 1
id = "rows"
kind = "format"

[[formats]]
name = "rows"
suffixes = ["rows"]
read = "read_csv({{path}}, delim='|')"
[formats.write]
kind = "copy"
options = "FORMAT CSV, DELIMITER '|', HEADER"

[[formats]]
name = "rows-no-header"
[formats.write]
kind = "copy"
options = "FORMAT CSV, DELIMITER '|', HEADER false"
```

Named formats use `/dev/stdin` for reading and `/dev/stdout` for writing. File paths and URLs use the endpoint path. Use `{{path}}` without surrounding quotes: dq supplies an escaped SQL string literal. This placeholder works in readers, COPY options, and setup SQL.

For read-only formats omit `write`; for write-only formats omit `read`. For Duckbox output, replace the writer table with `[formats.write]` and `kind = "duckbox"`. Terminal auto-display selects the registered `pretty` format.

## Setup steps

An extension step loads the extension, automatically installing it if needed. Omit `repository` for DuckDB's default repository; repository values use DuckDB's `INSTALL ... FROM` SQL syntax. A YAML format using a community extension would add:

```toml
[[setup]]
kind = "extension"
name = "yaml"
repository = "community"
```

Use SQL steps for connection configuration. Environment substitutions belong in setup SQL and also become escaped SQL string literals:

```toml
[[setup]]
kind = "sql"
requires_env = ["DQ_CA_CERT_FILE"]
sql = "SET ca_cert_file = {{env:DQ_CA_CERT_FILE}};"
```

`requires_env` skips the whole step if any listed variable is absent. An unguarded missing environment substitution fails execution. Environment values come from the executing process, rather than being captured in the source plan. Extension names and repositories are SQL syntax, not placeholder templates. Setup has no query-result conditions.

Setup runs on the DuckDB connection executing the pipeline, including terminal auto-display. Source recipes travel with the plan and transforms preserve them. Storage setup precedes format setup at each endpoint; source setup precedes destination setup. Steps run in manifest order, once per plugin ID per connection. Different setup recipes with the same ID fail.

Write SQL for one initialization per connection. If setup uses `{{path}}`, it receives the first endpoint selecting the plugin, even when both endpoints use it. `dq sql` prints compiled SQL without executing setup or reading data through DuckDB. `dq install` saves recipes without executing setup.

## Storage example

This complete storage plugin supports S3. Disable the bundled `s3` ID before installing it, as shown under configuration:

```toml
api_version = 1
id = "s3"
kind = "storage"
schemes = ["s3"]

[[setup]]
kind = "extension"
name = "httpfs"

[[setup]]
kind = "extension"
name = "aws"

[[setup]]
kind = "sql"
sql = """
CREATE TEMPORARY SECRET dq_plugin_s3 (
    TYPE s3,
    PROVIDER credential_chain,
    VALIDATION 'none',
    REFRESH auto
);
"""
```

Storage and format plugins compose: `dq from s3://bucket/input.rows | dq to s3://bucket/output.rows` uses S3 setup and the `rows` recipes at both endpoints, with shared setup executed once.

## Installation and configuration

```bash
dq install https://github.com/owner/repo/blob/main/rows.toml
dq install --replace https://github.com/owner/repo/blob/main/rows.toml
```

GitHub file URLs are converted to raw downloads; other HTTP/HTTPS full-file URLs work directly. The URL can select a branch, tag, or commit. Installation validates the manifest and registration conflicts, saves it as `<id>.toml`, and replaces existing user plugins only with `--replace`.

Global configuration lives under `$XDG_CONFIG_HOME/dq`, or `~/.config/dq` when `XDG_CONFIG_HOME` is unset or empty. dq automatically loads `plugins/*.toml` there; local manifests can be copied directly into that directory. Optional `config.toml` controls bundled IDs and additional directories:

```toml
disabled_bundled = ["s3"]
plugin_dirs = ["extra", "/path/to/shared/plugins"]
```

Relative directories resolve against the dq configuration directory. Disabling an ID removes all of that bundled plugin's variants; a user plugin can then reuse its ID and claims. Disabling `json` removes both `json` and `json-array`. Preserve a `pretty` registration when terminal auto-display is needed.

## Isolated validation

For the `rows.toml` example, run this with dq installed and the manifest in the current directory:

```bash
set -euo pipefail
temp_dir=$(mktemp -d)
trap 'rm -rf "$temp_dir"' EXIT
export XDG_CONFIG_HOME="$temp_dir"
mkdir -p "$XDG_CONFIG_HOME/dq/plugins"
cp rows.toml "$XDG_CONFIG_HOME/dq/plugins/rows.toml"
printf 'name|age\nAda|37\n' > "$temp_dir/input.rows"
printf '{"name":"Ada","age":37}\n' > "$temp_dir/expected.json"

dq from "$temp_dir/input.rows" | dq to json
cat "$temp_dir/input.rows" | dq from rows | dq to json
dq from json < "$temp_dir/expected.json" | dq to "$temp_dir/output.rows"
dq from "$temp_dir/output.rows" | dq to json
dq from json < "$temp_dir/expected.json" | dq to rows > "$temp_dir/stdout.rows"
dq from "$temp_dir/stdout.rows" | dq to json
dq from json < "$temp_dir/expected.json" | dq to rows-no-header
dq from "$temp_dir/input.rows" | dq sql
```

The JSON results should contain Ada's row, `rows-no-header` should output `Ada|37`, and the compiled SQL should use `read_csv` with `delim='|'`. For one-way formats, check that the opposite direction reports unsupported reading or writing. For shared setup, use one pipeline selecting the plugin at both ends and observe initialization through a query result.

Check URL sources and destinations for storage plugins with available credentials and infrastructure. Report unavailable infrastructure as a validation gap. Record reader/extension limitations rather than adding stdin materialization to dq; stdin remains single-pass. YAML stdin currently fails in the DuckDB YAML extension, so use YAML file paths for input.
