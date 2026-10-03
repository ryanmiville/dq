use std::{
    collections::HashMap,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use duckdb::Connection;
use serde::{Deserialize, Serialize};
use ureq::tls::{RootCerts, TlsConfig};

use crate::extensions::load_or_install;

include!(concat!(env!("OUT_DIR"), "/bundled_plugins.rs"));

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plugin {
    api_version: u32,
    id: String,
    kind: PluginKind,
    #[serde(default)]
    formats: Vec<Format>,
    #[serde(default)]
    schemes: Vec<String>,
    #[serde(default)]
    setup: Vec<Step>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PluginKind {
    Format,
    Storage,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Format {
    pub name: String,
    #[serde(default)]
    suffixes: Vec<String>,
    pub read: Option<String>,
    pub write: Option<Writer>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Writer {
    Copy { options: String },
    Duckbox,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Setup {
    id: String,
    pub path: String,
    steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Step {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    requires_env: Vec<String>,
    #[serde(flatten)]
    action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Extension {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repository: Option<String>,
    },
    Sql {
        sql: String,
    },
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[serde(default)]
    disabled_bundled: Vec<String>,
    #[serde(default)]
    plugin_dirs: Vec<PathBuf>,
}

pub struct Registry {
    plugins: Vec<Plugin>,
    names: HashMap<String, (usize, usize)>,
    suffixes: HashMap<String, (usize, usize)>,
    schemes: HashMap<String, usize>,
}

impl Registry {
    pub fn load() -> Result<Self> {
        Self::load_excluding(None)
    }

    fn load_excluding(excluded: Option<&Path>) -> Result<Self> {
        let root = config_directory()?;
        let config_path = root.join("config.toml");
        let config = match fs::read_to_string(&config_path) {
            Ok(text) => toml::from_str::<Config>(&text)
                .with_context(|| format!("invalid dq config {}", config_path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read {}", config_path.display()));
            }
        };
        let mut plugins = Vec::new();
        for text in BUNDLED {
            let plugin = parse_plugin(text).context("invalid bundled plugin")?;
            if !config.disabled_bundled.contains(&plugin.id) {
                plugins.push(plugin);
            }
        }
        for (directory, optional) in std::iter::once((root.join("plugins"), true)).chain(
            config
                .plugin_dirs
                .into_iter()
                .map(|path| (root.join(path), false)),
        ) {
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) if optional && error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to read plugin directory {}", directory.display())
                    });
                }
            };
            let mut paths = entries
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            paths.sort();
            for path in paths {
                if excluded.is_some_and(|excluded| path == excluded) {
                    continue;
                }
                if path
                    .extension()
                    .is_some_and(|extension| extension == "toml")
                {
                    let text = fs::read_to_string(&path)
                        .with_context(|| format!("failed to read plugin {}", path.display()))?;
                    plugins.push(
                        parse_plugin(&text)
                            .with_context(|| format!("invalid plugin {}", path.display()))?,
                    );
                }
            }
        }
        Self::new(plugins)
    }

    fn new(plugins: Vec<Plugin>) -> Result<Self> {
        let mut registry = Self {
            plugins,
            names: HashMap::new(),
            suffixes: HashMap::new(),
            schemes: HashMap::new(),
        };
        let mut ids = HashMap::new();
        for (index, plugin) in registry.plugins.iter().enumerate() {
            if ids.insert(&plugin.id, index).is_some() {
                bail!("duplicate plugin id `{}`", plugin.id);
            }
            for (format_index, format) in plugin.formats.iter().enumerate() {
                claim(
                    &mut registry.names,
                    &registry.plugins,
                    &format.name,
                    (index, format_index),
                    "format name",
                )?;
                for suffix in &format.suffixes {
                    claim(
                        &mut registry.suffixes,
                        &registry.plugins,
                        suffix,
                        (index, format_index),
                        "suffix",
                    )?;
                }
            }
            for scheme in &plugin.schemes {
                if let Some(previous) = registry.schemes.insert(scheme.to_ascii_lowercase(), index)
                {
                    bail!(
                        "conflicting scheme `{scheme}`: plugins `{}` and `{}`",
                        registry.plugins[previous].id,
                        plugin.id
                    );
                }
            }
        }
        Ok(registry)
    }

    pub fn named(&self, name: &str) -> Option<(&Plugin, &Format)> {
        self.selection(self.names.get(&name.to_ascii_lowercase()))
    }

    pub fn for_path(&self, path: &str) -> Option<(&Plugin, &Format)> {
        let path = if path.contains("://") {
            path.split(['?', '#']).next().unwrap_or(path)
        } else {
            path
        };
        let suffix = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
        self.selection(self.suffixes.get(&suffix))
    }

    fn selection(&self, selection: Option<&(usize, usize)>) -> Option<(&Plugin, &Format)> {
        selection
            .map(|&(plugin, format)| (&self.plugins[plugin], &self.plugins[plugin].formats[format]))
    }

    pub fn setup_for(&self, path: &str, format_plugin: Option<&Plugin>) -> Vec<Setup> {
        let storage = path
            .split_once("://")
            .and_then(|(scheme, _)| self.schemes.get(&scheme.to_ascii_lowercase()))
            .map(|&index| &self.plugins[index]);
        storage
            .into_iter()
            .chain(format_plugin)
            .filter(|plugin| !plugin.setup.is_empty())
            .map(|plugin| Setup {
                id: plugin.id.clone(),
                path: path.into(),
                steps: plugin.setup.clone(),
            })
            .collect()
    }

    pub fn setup_for_expression(&self, expression: &str) -> Result<Vec<Setup>> {
        for plugin in &self.plugins {
            for format in &plugin.formats {
                if let Some(read) = &format.read
                    && bind_path(read, "/dev/stdin")? == expression
                {
                    return Ok(self.setup_for("/dev/stdin", Some(plugin)));
                }
            }
        }
        Ok(Vec::new())
    }
}

fn config_directory() -> Result<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|root| root.join("dq"))
        .ok_or_else(|| anyhow!("cannot locate global dq config directory"))
}

pub fn install(url: &str, replace: bool) -> Result<()> {
    let url = download_url(url)?;
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent();
    let text = agent
        .get(&url)
        .call()
        .with_context(|| format!("failed to download plugin from {url}"))?
        .body_mut()
        .read_to_string()
        .context("failed to read downloaded plugin")?;
    let plugin = parse_plugin(&text).context("invalid downloaded plugin")?;
    let id = plugin.id.clone();
    if id == "."
        || id == ".."
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        bail!(
            "installed plugin id must use letters, digits, '.', '-', or '_' and must not be '.' or '..'"
        );
    }
    let directory = config_directory()?.join("plugins");
    let path = directory.join(format!("{id}.toml"));
    if path.try_exists()? && !replace {
        bail!(
            "user plugin `{id}` already exists at {}; use --replace to update it",
            path.display()
        );
    }
    let mut plugins = Registry::load_excluding(Some(&path))?.plugins;
    plugins.push(plugin);
    Registry::new(plugins).context(
        "plugin registration conflict; disable conflicting bundled plugins with disabled_bundled in config.toml",
    )?;
    fs::create_dir_all(&directory)
        .with_context(|| format!("failed to create plugin directory {}", directory.display()))?;
    let mut file = tempfile::NamedTempFile::new_in(&directory)?;
    file.write_all(text.as_bytes())?;
    file.as_file().sync_all()?;
    let installed = if replace {
        file.persist(&path)
    } else {
        file.persist_noclobber(&path)
    };
    installed.with_context(|| format!("failed to install plugin `{id}` at {}", path.display()))?;
    println!("Installed plugin `{id}` at {}", path.display());
    Ok(())
}

fn download_url(url: &str) -> Result<String> {
    let url = url.split('#').next().unwrap_or(url);
    let uri: ureq::http::Uri = url
        .parse()
        .context("expected an http or https URL pointing to a plugin TOML file")?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.host().is_none() {
        bail!("expected an http or https URL pointing to a plugin TOML file");
    }
    if uri.host().is_some_and(|host| {
        host.eq_ignore_ascii_case("github.com") || host.eq_ignore_ascii_case("www.github.com")
    }) {
        let mut parts = uri.path().trim_start_matches('/').splitn(4, '/');
        match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(owner), Some(repository), Some("blob" | "raw"), Some(file))
                if !owner.is_empty()
                    && !repository.is_empty()
                    && file.split_once('/').is_some_and(|(reference, path)| {
                        !reference.is_empty() && !path.is_empty()
                    }) =>
            {
                return Ok(format!(
                    "https://raw.githubusercontent.com/{owner}/{repository}/{file}"
                ));
            }
            (Some(owner), Some(repository), Some(marker), Some(file))
                if !owner.is_empty()
                    && !repository.is_empty()
                    && !matches!(marker, "blob" | "raw")
                    && !file.is_empty() => {}
            _ => bail!(
                "GitHub URL must point to a file: https://github.com/owner/repo/blob/ref/plugin.toml"
            ),
        }
    }
    Ok(url.into())
}

fn claim(
    claims: &mut HashMap<String, (usize, usize)>,
    plugins: &[Plugin],
    key: &str,
    value: (usize, usize),
    label: &str,
) -> Result<()> {
    if let Some((previous, _)) = claims.insert(key.to_ascii_lowercase(), value) {
        bail!(
            "conflicting {label} `{key}`: plugins `{}` and `{}`; disable a bundled plugin in config.toml",
            plugins[previous].id,
            plugins[value.0].id
        );
    }
    Ok(())
}

fn parse_plugin(text: &str) -> Result<Plugin> {
    let plugin: Plugin = toml::from_str(text)?;
    if plugin.api_version != 1 {
        bail!("unsupported plugin API version {}", plugin.api_version);
    }
    if plugin.id.trim().is_empty() {
        bail!("plugin id must not be empty");
    }
    match plugin.kind {
        PluginKind::Format if plugin.formats.is_empty() || !plugin.schemes.is_empty() => {
            bail!("format plugin must declare formats and no schemes")
        }
        PluginKind::Storage if plugin.schemes.is_empty() || !plugin.formats.is_empty() => {
            bail!("storage plugin must declare schemes and no formats")
        }
        _ => {}
    }
    for format in &plugin.formats {
        if format.name.trim().is_empty() || (format.read.is_none() && format.write.is_none()) {
            bail!("format must have a name and a read or write recipe");
        }
    }
    Ok(plugin)
}

pub fn sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn expand(template: &str, mut value: impl FnMut(&str) -> Result<String>) -> Result<String> {
    let mut result = String::new();
    let mut remaining = template;
    while let Some((prefix, rest)) = remaining.split_once("{{") {
        result.push_str(prefix);
        let (key, tail) = rest.split_once("}}").context("unclosed SQL placeholder")?;
        result.push_str(&sql_literal(&value(key)?));
        remaining = tail;
    }
    result.push_str(remaining);
    Ok(result)
}

pub fn bind_path(template: &str, path: &str) -> Result<String> {
    expand(template, |key| match key {
        "path" => Ok(path.into()),
        _ => bail!(
            "unknown reader/writer placeholder `{key}`; environment placeholders belong in setup SQL"
        ),
    })
}

pub fn prepare<'a>(conn: &Connection, setup: impl IntoIterator<Item = &'a Setup>) -> Result<()> {
    let mut prepared: HashMap<&str, &[Step]> = HashMap::new();
    for plugin in setup {
        if let Some(steps) = prepared.insert(&plugin.id, &plugin.steps) {
            if steps != plugin.steps.as_slice() {
                bail!("conflicting setup recipes for plugin `{}`", plugin.id);
            }
            continue;
        }
        for (index, step) in plugin.steps.iter().enumerate() {
            if step
                .requires_env
                .iter()
                .any(|name| env::var_os(name).is_none())
            {
                continue;
            }
            let context = || format!("plugin `{}` setup step {} failed", plugin.id, index + 1);
            match &step.action {
                Action::Sql { sql } => {
                    let sql = expand(sql, |key| {
                        if key == "path" {
                            Ok(plugin.path.clone())
                        } else if let Some(variable) = key.strip_prefix("env:") {
                            env::var(variable)
                                .with_context(|| format!("missing environment variable {variable}"))
                        } else {
                            bail!("unknown setup placeholder `{key}`")
                        }
                    })
                    .with_context(context)?;
                    conn.execute_batch(&sql).with_context(context)?;
                }
                Action::Extension { name, repository } => {
                    load_or_install(conn, name, repository.as_deref()).with_context(context)?;
                }
            }
        }
    }
    Ok(())
}
