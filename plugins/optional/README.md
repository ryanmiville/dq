# Optional plugins

These plugins are maintained in this repository and installed separately. They are not embedded in the dq binary. Install a plugin using the full URL to its TOML file; use `--replace` to update an existing installation. The URL can select a branch, tag, or commit.

## YAML

```bash
dq install https://github.com/ryanmiville/dq/blob/main/plugins/optional/yaml.toml
```

The [YAML plugin](yaml.toml) uses DuckDB's community `yaml` extension, which dq automatically installs and loads when executing a YAML pipeline.

It reads `.yaml` and `.yml` files and writes YAML to files or stdout. Reading YAML from stdin currently fails in the DuckDB extension, so the plugin remains optional until that is fixed.

```bash
dq from config.yml | dq to json
dq from data.json | dq to yaml
```

To add an optional plugin, place its TOML manifest in this directory and document its installation URL, supported formats, and limitations here. Files in `plugins/bundled/` are embedded at build time.
