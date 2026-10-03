use std::{env, fs, path::Path};

use anyhow::{Context, Result, bail};

use crate::{
    plan::Source,
    plugins::{Format, Plugin, Registry, Setup, Writer, bind_path, sql_literal},
};

pub struct InputFormat {
    pub source: Source,
    pub setup: Vec<Setup>,
}

pub struct OutputFormat {
    pub execution: OutputExecution,
    pub setup: Vec<Setup>,
}

pub enum OutputExecution {
    Copy(String),
    Pretty(String),
}

impl InputFormat {
    pub fn parse(value: Option<String>, expr: Option<String>) -> Result<Self> {
        let registry = Registry::load()?;
        match (value, expr) {
            (_, Some(read_expr)) => {
                let setup = registry.setup_for_expression(&read_expr)?;
                Ok(Self {
                    source: Source::Stream { read_expr },
                    setup,
                })
            }
            (Some(value), None) => {
                if let Some((plugin, format)) = registry.named(&value) {
                    let read = format.read.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("format `{}` does not support reading", format.name)
                    })?;
                    return Ok(Self {
                        source: Source::Stream {
                            read_expr: bind_path(read, "/dev/stdin")?,
                        },
                        setup: registry.setup_for("/dev/stdin", Some(plugin)),
                    });
                }
                if file_like(&value) || value.contains("://") {
                    let format = registry.for_path(&value);
                    if let Some((_, format)) = format
                        && format.read.is_none()
                    {
                        bail!("format `{}` does not support reading", format.name);
                    }
                    let path = if value.contains("://") {
                        value
                    } else {
                        resolve_existing_path(&value)?
                    };
                    let read_expr = format
                        .and_then(|(_, format)| format.read.as_ref())
                        .map(|read| bind_path(read, &path))
                        .transpose()?;
                    let setup = registry.setup_for(&path, format.map(|(plugin, _)| plugin));
                    return Ok(Self {
                        source: Source::Path { path, read_expr },
                        setup,
                    });
                }
                let setup = registry.setup_for_expression(&value)?;
                Ok(Self {
                    source: Source::Stream { read_expr: value },
                    setup,
                })
            }
            (None, None) => unreachable!("clap guarantees either a positional value or --expr"),
        }
    }
}

impl OutputFormat {
    pub fn parse(value: Option<String>, expr: Option<String>) -> Result<Self> {
        let registry = Registry::load()?;
        match (value, expr) {
            (_, Some(expr)) => Ok(Self {
                execution: OutputExecution::Copy(expr),
                setup: Vec::new(),
            }),
            (Some(value), None) => {
                let named = registry.named(&value);
                let file = file_like(&value) || value.contains("://");
                if named.is_none() && !file {
                    return Ok(Self {
                        execution: OutputExecution::Copy(value),
                        setup: Vec::new(),
                    });
                }
                let path = if named.is_some() {
                    "/dev/stdout"
                } else {
                    &value
                };
                let format = named.or_else(|| registry.for_path(&value));
                Self::resolve(&registry, path, format)
            }
            (None, None) => unreachable!("clap guarantees either a positional value or --expr"),
        }
    }

    pub fn terminal() -> Result<Self> {
        let registry = Registry::load()?;
        let format = registry.named("pretty").ok_or_else(|| {
            anyhow::anyhow!("terminal output requires a registered `pretty` format")
        })?;
        Self::resolve(&registry, "/dev/stdout", Some(format))
    }

    fn resolve(
        registry: &Registry,
        path: &str,
        format: Option<(&Plugin, &Format)>,
    ) -> Result<Self> {
        let execution = match format {
            Some((_, format)) => match format.write.as_ref() {
                Some(Writer::Copy { options }) => OutputExecution::Copy(format!(
                    "{} ({})",
                    sql_literal(path),
                    bind_path(options, path)?
                )),
                Some(Writer::Duckbox) => OutputExecution::Pretty(path.into()),
                None => bail!("format `{}` does not support writing", format.name),
            },
            None => OutputExecution::Copy(sql_literal(path)),
        };
        Ok(Self {
            execution,
            setup: registry.setup_for(path, format.map(|(plugin, _)| plugin)),
        })
    }
}

fn file_like(value: &str) -> bool {
    Path::new(value).extension().is_some()
}

fn resolve_existing_path(path: &str) -> Result<String> {
    let absolute = if path.contains(['*', '?', '[']) {
        env::current_dir()?.join(path)
    } else {
        fs::canonicalize(Path::new(path))
            .with_context(|| format!("failed to resolve input path `{path}`"))?
    };
    Ok(absolute.to_string_lossy().into_owned())
}
