//! The provider catalog: the built-in provider files, overridden or added to by a folder of the user's.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::{fmt, fs, io};

use serde::Deserialize;
use serde_json::{Map, Value};
use toml::Table;

include!(concat!(env!("OUT_DIR"), "/providers.rs"));

/// The wire format a provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wire {
    /// Chat completions.
    Openai,
    /// Messages.
    Anthropic,
}

/// What happens to earlier thinking on the next request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reasoning {
    /// It is stored and not re-sent.
    #[default]
    Drop,
    /// It goes back to the model.
    Echo,
}

/// How the provider's prompt cache is used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheMode {
    /// The provider caches by prefix and pard sends nothing.
    #[default]
    Auto,
    /// pard marks the cached prefix.
    Marked,
}

/// How long a marked cache entry lives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum Ttl {
    /// Five minutes.
    #[default]
    #[serde(rename = "5m")]
    Minutes5,
    /// One hour.
    #[serde(rename = "1h")]
    Hour1,
}

/// A provider's cache settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cache {
    /// Whether pard marks the prefix.
    pub mode: CacheMode,
    /// The life of a marked entry.
    pub ttl: Ttl,
}

/// Key paths into the response's usage object, where a provider's names differ from the wire's own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UsagePaths {
    /// The cached input tokens.
    pub cache_read: Option<Vec<String>>,
    /// The input tokens written to the cache.
    pub cache_write: Option<Vec<String>>,
    /// The call's cost.
    pub cost: Option<Vec<String>>,
}

/// The error texts that carry a meaning.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Errors {
    /// A 400 whose body holds one of these means the context is full.
    pub overflow: Vec<String>,
}

/// A model's prices per million tokens.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Price {
    /// The currency code.
    pub currency: String,
    /// Uncached input.
    pub input: f64,
    /// Input read from the cache.
    pub cache_read: f64,
    /// Input written to the cache.
    pub cache_write: f64,
    /// Output.
    pub output: f64,
}

/// One model of a provider.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    /// The provider's id for it.
    pub id: String,
    /// Its display name.
    pub name: String,
    /// Its context window in tokens.
    pub context: u64,
    /// The most output tokens a request asks for.
    pub max_output: u64,
    /// Its prices, where known.
    pub price: Option<Price>,
    /// Fields merged into every request for it.
    #[serde(default)]
    pub body: Map<String, Value>,
}

/// One provider, as its file describes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provider {
    /// The provider's file name, the first part of its models' ids.
    #[serde(skip)]
    pub id: String,
    /// Its display name.
    pub name: String,
    /// The wire format it speaks.
    pub wire: Wire,
    /// The URL the wire's path is added to.
    pub base_url: String,
    /// The environment variable the key arrives in.
    pub key_env: String,
    /// The header that carries the key, `{key}` standing for it.
    pub auth: String,
    /// Fixed extra headers.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Whether earlier thinking is re-sent.
    #[serde(default)]
    pub reasoning: Reasoning,
    /// The cache settings.
    #[serde(default)]
    pub cache: Cache,
    /// Where the usage object keeps its counts.
    #[serde(default)]
    pub usage: UsagePaths,
    /// The error texts that carry a meaning.
    #[serde(default)]
    pub errors: Errors,
    /// Its models.
    pub models: Vec<Model>,
}

/// Why the catalog could not load, or has no such model.
#[derive(Debug)]
pub enum CatalogError {
    /// The user's provider folder or a file in it could not be read.
    Read {
        /// The path that failed.
        path: PathBuf,
        /// The error.
        error: io::Error,
    },
    /// A provider file is not a valid provider.
    Invalid {
        /// The file, or the built-in's name.
        file: String,
        /// What is wrong with it.
        reason: String,
    },
    /// A model id names no provider file or no model in it.
    Unknown(String),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, error } => write!(f, "provider file {}: {error}", path.display()),
            Self::Invalid { file, reason } => write!(f, "provider file {file}: {reason}"),
            Self::Unknown(id) => {
                write!(f, "unknown model {id}: expected <provider file>/<model id>")
            }
        }
    }
}

impl std::error::Error for CatalogError {}

/// Every provider pard knows, by file name.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    providers: BTreeMap<String, Provider>,
}

impl Catalog {
    /// Loads the built-in providers, then the user's `<name>.toml` files in `dir`, which override a built-in key by key or add a provider.
    ///
    /// # Errors
    ///
    /// A folder or file that cannot be read, or a file that is not a valid provider.
    pub fn load(dir: &Path) -> Result<Self, CatalogError> {
        let mut tables = BTreeMap::new();
        for (name, text) in BUILT_IN {
            tables.insert(
                (*name).to_owned(),
                parse(&format!("{name}.toml (built in)"), text)?,
            );
        }
        for (name, path) in user_files(dir)? {
            let text = fs::read_to_string(&path).map_err(|error| CatalogError::Read {
                path: path.clone(),
                error,
            })?;
            let table = parse(&path.display().to_string(), &text)?;
            merge(tables.entry(name).or_default(), table);
        }
        let mut providers = BTreeMap::new();
        for (name, table) in tables {
            let provider = provider(&format!("{name}.toml"), name.clone(), table)?;
            providers.insert(name, provider);
        }
        Ok(Self { providers })
    }

    /// The providers, by file name.
    pub fn providers(&self) -> impl Iterator<Item = &Provider> {
        self.providers.values()
    }

    /// The provider and model of an id `<provider file>/<model id>`.
    ///
    /// # Errors
    ///
    /// No provider file of that name, or no such model in it.
    pub fn model(&self, id: &str) -> Result<(&Provider, &Model), CatalogError> {
        let unknown = || CatalogError::Unknown(id.to_owned());
        let (file, model) = id.split_once('/').ok_or_else(unknown)?;
        let provider = self.providers.get(file).ok_or_else(unknown)?;
        let model = provider
            .models
            .iter()
            .find(|m| m.id == model)
            .ok_or_else(unknown)?;
        Ok((provider, model))
    }
}

fn user_files(dir: &Path) -> Result<Vec<(String, PathBuf)>, CatalogError> {
    let read_error = |error| CatalogError::Read {
        path: dir.to_owned(),
        error,
    };
    let entries = match fs::read_dir(dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        entries => entries.map_err(read_error)?,
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(read_error)?.path();
        if path.extension().is_some_and(|x| x == "toml") {
            let name = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned);
            let name = name.ok_or_else(|| {
                invalid(&path.display().to_string(), "the file name is not UTF-8")
            })?;
            files.push((name, path));
        }
    }
    Ok(files)
}

fn parse(file: &str, text: &str) -> Result<Table, CatalogError> {
    toml::from_str(text).map_err(|e| invalid(file, &e.to_string()))
}

fn merge(base: &mut Table, over: Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(old)), toml::Value::Table(new)) => merge(old, new),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

fn provider(file: &str, id: String, table: Table) -> Result<Provider, CatalogError> {
    let mut provider: Provider = toml::Value::Table(table)
        .try_into()
        .map_err(|e: toml::de::Error| invalid(file, &e.to_string()))?;
    if !provider.auth.contains("{key}") || !provider.auth.contains(':') {
        return Err(invalid(
            file,
            "auth must be `<header>: <value>` with {key} in the value",
        ));
    }
    if provider.models.is_empty() {
        return Err(invalid(file, "no [[models]]"));
    }
    let mut ids = BTreeSet::new();
    if let Some(model) = provider.models.iter().find(|m| !ids.insert(m.id.as_str())) {
        return Err(invalid(
            file,
            &format!("model {} is listed twice", model.id),
        ));
    }
    provider.id = id;
    Ok(provider)
}

fn invalid(file: &str, reason: &str) -> CatalogError {
    CatalogError::Invalid {
        file: file.to_owned(),
        reason: reason.to_owned(),
    }
}
