mod class_name_pattern;
mod parse;

use std::{
    borrow::Cow,
    fs,
    hash::{Hash as _, Hasher as _},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context};
use class_name_pattern::ClassNamePattern;
use parse::{CssFragment, Global};
use serde::Deserialize;
use siphasher::sip::SipHasher13;

fn default_extensions() -> Vec<String> {
    vec![".module.css".to_owned(), ".module.scss".to_owned()]
}

fn default_folders() -> Vec<PathBuf> {
    vec![PathBuf::from_str("./src/").expect("path is valid")]
}

fn default_hash_len() -> usize {
    7
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub output_file: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    #[serde(default = "default_extensions")]
    pub extensions: Vec<String>,
    #[serde(default = "default_folders")]
    pub folders: Vec<PathBuf>,
    pub scss_prelude: Option<String>,
    #[serde(default = "default_hash_len")]
    pub hash_len: usize,
    #[serde(default)]
    pub class_name_pattern: ClassNamePattern,
    pub hash_root_path: Option<PathBuf>,
    #[serde(default)]
    pub workspace: bool,
}

/// Raw config with all fields optional, used for both workspace-level
/// and crate-level parsing so we can distinguish "not set" from "set to default".
#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct RawConfig {
    pub output_file: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub extensions: Option<Vec<String>>,
    pub folders: Option<Vec<PathBuf>>,
    pub scss_prelude: Option<String>,
    pub hash_len: Option<usize>,
    pub class_name_pattern: Option<ClassNamePattern>,
    pub hash_root_path: Option<PathBuf>,
    #[serde(default)]
    pub workspace: bool,
}

impl RawConfig {
    fn into_config(self) -> Config {
        Config {
            output_file: self.output_file,
            output_dir: self.output_dir,
            extensions: self.extensions.unwrap_or_else(default_extensions),
            folders: self.folders.unwrap_or_else(default_folders),
            scss_prelude: self.scss_prelude,
            hash_len: self.hash_len.unwrap_or_else(default_hash_len),
            class_name_pattern: self.class_name_pattern.unwrap_or_default(),
            hash_root_path: self.hash_root_path,
            workspace: self.workspace,
        }
    }

    /// Merge workspace raw config as defaults under crate raw config.
    /// Crate-level explicit values take precedence.
    fn merge_workspace(self, ws: &RawConfig, manifest_dir: &Path, workspace_root: &Path) -> Config {
        let hash_root_path = self
            .hash_root_path
            .or_else(|| ws.hash_root_path.clone())
            .or_else(|| pathdiff::diff_paths(workspace_root, manifest_dir));

        Config {
            output_file: self.output_file.or_else(|| ws.output_file.clone()),
            output_dir: self.output_dir.or_else(|| ws.output_dir.clone()),
            extensions: self
                .extensions
                .or_else(|| ws.extensions.clone())
                .unwrap_or_else(default_extensions),
            folders: self
                .folders
                .or_else(|| ws.folders.clone())
                .unwrap_or_else(default_folders),
            scss_prelude: self.scss_prelude.or_else(|| ws.scss_prelude.clone()),
            hash_len: self
                .hash_len
                .or(ws.hash_len)
                .unwrap_or_else(default_hash_len),
            class_name_pattern: self
                .class_name_pattern
                .or_else(|| ws.class_name_pattern.clone())
                .unwrap_or_default(),
            hash_root_path,
            workspace: true,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_file: None,
            output_dir: None,
            extensions: default_extensions(),
            folders: default_folders(),
            scss_prelude: None,
            hash_len: default_hash_len(),
            class_name_pattern: Default::default(),
            hash_root_path: None,
            workspace: false,
        }
    }
}

#[derive(Deserialize)]
struct CargoToml {
    package: Option<CargoTomlPackage>,
    workspace: Option<CargoTomlWorkspace>,
}

#[derive(Deserialize)]
struct CargoTomlPackage {
    metadata: Option<CargoTomlPackageMetadata>,
    /// Explicit workspace path, e.g. `workspace = "../my-workspace"`
    workspace: Option<toml::Value>,
}

#[derive(Deserialize)]
struct CargoTomlPackageMetadata {
    stylance: Option<RawConfig>,
}

#[derive(Deserialize)]
struct CargoTomlWorkspace {
    metadata: Option<CargoTomlWorkspaceMetadata>,
}

#[derive(Deserialize)]
struct CargoTomlWorkspaceMetadata {
    stylance: Option<RawConfig>,
}

pub fn hash_string(input: &str) -> u64 {
    let mut hasher = SipHasher13::new();
    input.hash(&mut hasher);
    hasher.finish()
}

pub struct Class {
    pub original_name: String,
    pub hashed_name: String,
}

/// Find the workspace root directory for a given crate manifest dir.
///
/// First checks if the crate's Cargo.toml has an explicit `[package] workspace` field.
/// Otherwise, walks up the directory tree looking for a Cargo.toml with a `[workspace]` section.
fn find_workspace_root(manifest_dir: &Path) -> anyhow::Result<PathBuf> {
    let cargo_toml_contents =
        fs::read_to_string(manifest_dir.join("Cargo.toml")).context("Failed to read Cargo.toml")?;
    let cargo_toml: CargoToml = toml::from_str(&cargo_toml_contents)?;

    // Check for explicit workspace path in [package] workspace = "path"
    if let Some(CargoTomlPackage {
        workspace: Some(toml::Value::String(workspace_path)),
        ..
    }) = &cargo_toml.package
    {
        return Ok(manifest_dir.join(workspace_path));
    }

    // Walk up looking for a Cargo.toml with [workspace]
    let mut current = manifest_dir.to_path_buf();
    loop {
        if !current.pop() {
            bail!(
                "Could not find workspace root for {}. \
                 No parent Cargo.toml with [workspace] was found.",
                manifest_dir.display()
            );
        }

        let candidate = current.join("Cargo.toml");
        if candidate.exists() {
            let contents = fs::read_to_string(&candidate)
                .with_context(|| format!("Failed to read {}", candidate.display()))?;
            let parsed: CargoToml = toml::from_str(&contents)?;
            if parsed.workspace.is_some() {
                return Ok(current);
            }
        }
    }
}

/// Load the workspace stylance config from a workspace root directory.
fn load_workspace_config(workspace_root: &Path) -> anyhow::Result<Option<RawConfig>> {
    let cargo_toml_contents = fs::read_to_string(workspace_root.join("Cargo.toml"))
        .context("Failed to read workspace Cargo.toml")?;
    let cargo_toml: CargoToml = toml::from_str(&cargo_toml_contents)?;

    Ok(cargo_toml
        .workspace
        .and_then(|w| w.metadata)
        .and_then(|m| m.stylance))
}

pub fn load_config(manifest_dir: &Path) -> anyhow::Result<Config> {
    let cargo_toml_contents =
        fs::read_to_string(manifest_dir.join("Cargo.toml")).context("Failed to read Cargo.toml")?;
    let cargo_toml: CargoToml = toml::from_str(&cargo_toml_contents)?;

    let raw = match cargo_toml.package {
        Some(CargoTomlPackage {
            metadata:
                Some(CargoTomlPackageMetadata {
                    stylance: Some(raw),
                }),
            ..
        }) => raw,
        _ => RawConfig::default(),
    };

    let config = if raw.workspace {
        let workspace_root = find_workspace_root(manifest_dir)?;
        match load_workspace_config(&workspace_root)? {
            Some(ws_config) => raw.merge_workspace(&ws_config, manifest_dir, &workspace_root),
            None => {
                // workspace = true but no [workspace.metadata.stylance] found.
                // Still set hash_root_path to workspace root implicitly.
                let hash_root_path = raw
                    .hash_root_path
                    .clone()
                    .or_else(|| pathdiff::diff_paths(&workspace_root, manifest_dir));
                let mut config = raw.into_config();
                config.hash_root_path = hash_root_path;
                config
            }
        }
    } else {
        raw.into_config()
    };

    if config.extensions.iter().any(|e| e.is_empty()) {
        bail!("Stylance config extensions can't be empty strings");
    }

    Ok(config)
}

/// Normalize a path by resolving `.` and `..` components and making it
/// absolute, without following symlinks. This preserves logical paths through
/// symlinked directories.
fn normalize_path(path: &Path) -> anyhow::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    let mut components = Vec::new();
    for component in absolute.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            other => components.push(other),
        }
    }
    Ok(components.iter().collect())
}

fn normalized_relative_path(base: &Path, subpath: &Path) -> anyhow::Result<String> {
    let base = normalize_path(base)?;
    let subpath = normalize_path(subpath)?;

    let relative_path_str: String = subpath
        .strip_prefix(base)
        .context("css file should be inside manifest_dir")?
        .to_string_lossy()
        .into();

    #[cfg(target_os = "windows")]
    let relative_path_str = relative_path_str.replace('\\', "/");

    Ok(relative_path_str)
}

fn make_hash(hash_root: &Path, css_file: &Path, hash_len: usize) -> anyhow::Result<String> {
    let relative_path_str = normalized_relative_path(hash_root, css_file)?;

    let hash = hash_string(&relative_path_str);
    let mut hash_str = format!("{hash:x}");
    hash_str.truncate(hash_len);
    Ok(hash_str)
}

/// Resolve the effective hash root directory.
/// If `config.hash_root_path` is set, it is resolved relative to `manifest_dir`.
/// Otherwise, `manifest_dir` is used as the hash root.
pub fn resolve_hash_root(manifest_dir: &Path, config: &Config) -> PathBuf {
    match &config.hash_root_path {
        Some(hash_root_path) => manifest_dir.join(hash_root_path),
        None => manifest_dir.to_path_buf(),
    }
}

pub struct ModifyCssResult {
    pub path: PathBuf,
    pub normalized_path_str: String,
    pub hash: String,
    pub contents: String,
}

pub fn load_and_modify_css(
    hash_root: &Path,
    css_file: &Path,
    config: &Config,
) -> anyhow::Result<ModifyCssResult> {
    let hash_str = make_hash(hash_root, css_file, config.hash_len)?;
    let css_file_contents = fs::read_to_string(css_file)?;

    let fragments = parse::parse_css(&css_file_contents).map_err(|e| anyhow!("{e}"))?;

    let mut new_file = String::with_capacity(css_file_contents.len() * 2);
    let mut cursor = css_file_contents.as_str();

    for fragment in fragments {
        let (span, replace) = match fragment {
            CssFragment::Class(class) => (
                class,
                Cow::Owned(config.class_name_pattern.apply(class, &hash_str)),
            ),
            CssFragment::Global(Global { inner, outer }) => (outer, Cow::Borrowed(inner)),
        };

        let (before, after) = cursor.split_at(span.as_ptr() as usize - cursor.as_ptr() as usize);
        cursor = &after[span.len()..];
        new_file.push_str(before);
        new_file.push_str(&replace);
    }

    new_file.push_str(cursor);

    Ok(ModifyCssResult {
        path: css_file.to_owned(),
        normalized_path_str: normalized_relative_path(hash_root, css_file)?,
        hash: hash_str,
        contents: new_file,
    })
}

pub fn get_classes(
    hash_root: &Path,
    css_file: &Path,
    config: &Config,
) -> anyhow::Result<(String, Vec<Class>)> {
    let hash_str = make_hash(hash_root, css_file, config.hash_len)?;

    let css_file_contents = fs::read_to_string(css_file)?;

    let mut classes = parse::parse_css(&css_file_contents)
        .map_err(|e| anyhow!("{e}"))?
        .into_iter()
        .filter_map(|c| {
            if let CssFragment::Class(c) = c {
                Some(c)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    classes.sort();
    classes.dedup();

    Ok((
        hash_str.clone(),
        classes
            .into_iter()
            .map(|class| Class {
                original_name: class.to_owned(),
                hashed_name: config.class_name_pattern.apply(class, &hash_str),
            })
            .collect(),
    ))
}
