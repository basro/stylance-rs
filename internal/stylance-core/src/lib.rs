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

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
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

impl Config {
    pub fn extensions(&self) -> &[String] {
        static DEFAULT_EXTENSIONS: std::sync::LazyLock<Vec<String>> =
            std::sync::LazyLock::new(default_extensions);
        self.extensions.as_deref().unwrap_or(&DEFAULT_EXTENSIONS)
    }

    pub fn folders(&self) -> &[PathBuf] {
        static DEFAULT_FOLDERS: std::sync::LazyLock<Vec<PathBuf>> =
            std::sync::LazyLock::new(default_folders);
        self.folders.as_deref().unwrap_or(&DEFAULT_FOLDERS)
    }

    pub fn hash_len(&self) -> usize {
        self.hash_len.unwrap_or_else(default_hash_len)
    }

    pub fn class_name_pattern(&self) -> &ClassNamePattern {
        static DEFAULT_PATTERN: std::sync::LazyLock<ClassNamePattern> =
            std::sync::LazyLock::new(ClassNamePattern::default);
        self.class_name_pattern.as_ref().unwrap_or(&DEFAULT_PATTERN)
    }

    /// Merge workspace config as defaults under this (crate) config.
    /// Crate-level explicit values take precedence; returns a new Config.
    fn merged_with_workspace(self, ws: Config) -> Config {
        Config {
            output_file: self.output_file.or(ws.output_file),
            output_dir: self.output_dir.or(ws.output_dir),
            extensions: self.extensions.or(ws.extensions),
            folders: self.folders.or(ws.folders),
            scss_prelude: self.scss_prelude.or(ws.scss_prelude),
            hash_len: self.hash_len.or(ws.hash_len),
            class_name_pattern: self.class_name_pattern.or(ws.class_name_pattern),
            hash_root_path: self.hash_root_path.or(ws.hash_root_path),
            workspace: self.workspace,
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
    #[serde(rename = "workspace")]
    workspace_path: Option<toml::Value>,
}

#[derive(Deserialize)]
struct CargoTomlPackageMetadata {
    stylance: Option<Config>,
}

#[derive(Deserialize)]
struct CargoTomlWorkspace {
    metadata: Option<CargoTomlWorkspaceMetadata>,
}

#[derive(Deserialize)]
struct CargoTomlWorkspaceMetadata {
    stylance: Option<Config>,
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

/// Find the workspace root directory and its parsed CargoToml.
/// First checks if the crate's own Cargo.toml has `[workspace]` (root crate).
/// Then checks for an explicit `[package] workspace` field.
/// Otherwise, walks up the directory tree looking for a Cargo.toml with `[workspace]`.
fn find_workspace_root(
    manifest_dir: &Path,
    cargo_toml: CargoToml,
) -> anyhow::Result<(PathBuf, CargoToml)> {
    // The crate's own Cargo.toml has [workspace] — it is the workspace root
    if cargo_toml.workspace.is_some() {
        return Ok((manifest_dir.to_path_buf(), cargo_toml));
    }

    // Check for explicit workspace path in [package] workspace = "path"
    if let Some(CargoTomlPackage {
        workspace_path: Some(toml::Value::String(workspace_path)),
        ..
    }) = &cargo_toml.package
    {
        let ws_root = manifest_dir.join(workspace_path);
        let contents = fs::read_to_string(ws_root.join("Cargo.toml")).with_context(|| {
            format!(
                "Failed to read workspace Cargo.toml at {}",
                ws_root.display()
            )
        })?;
        let parsed: CargoToml = toml::from_str(&contents)?;
        return Ok((ws_root, parsed));
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
                return Ok((current, parsed));
            }
        }
    }
}

/// Compute a relative path from `base` to `target` using `..` components.
fn relative_path(base: &Path, target: &Path) -> anyhow::Result<PathBuf> {
    let base = normalize_path(base)?;
    let target = normalize_path(target)?;

    // On Windows, verify both paths are on the same drive
    #[cfg(target_os = "windows")]
    {
        use std::path::Component;
        let base_prefix = base.components().next();
        let target_prefix = target.components().next();
        if let (Some(Component::Prefix(a)), Some(Component::Prefix(b))) =
            (base_prefix, target_prefix)
        {
            if a.kind() != b.kind() {
                bail!(
                    "Cannot compute relative path between different drives: {} and {}",
                    base.display(),
                    target.display()
                );
            }
        }
    }

    let mut base_iter = base.components().peekable();
    let mut target_iter = target.components().peekable();

    // Skip common prefix
    while let (Some(a), Some(b)) = (base_iter.peek(), target_iter.peek()) {
        if a == b {
            base_iter.next();
            target_iter.next();
        } else {
            break;
        }
    }

    let mut result = PathBuf::new();
    for _ in base_iter {
        result.push("..");
    }
    for component in target_iter {
        result.push(component);
    }
    Ok(result)
}

pub fn load_config(manifest_dir: &Path) -> anyhow::Result<Config> {
    let cargo_toml_contents =
        fs::read_to_string(manifest_dir.join("Cargo.toml")).context("Failed to read Cargo.toml")?;
    let cargo_toml: CargoToml = toml::from_str(&cargo_toml_contents)?;

    let is_workspace = cargo_toml
        .package
        .as_ref()
        .and_then(|p| p.metadata.as_ref())
        .and_then(|m| m.stylance.as_ref())
        .is_some_and(|c| c.workspace);

    // Extract the crate config before passing ownership to find_workspace_root
    let mut config = cargo_toml
        .package
        .as_ref()
        .and_then(|p| p.metadata.as_ref())
        .and_then(|m| m.stylance.clone())
        .unwrap_or_default();

    if is_workspace {
        let (workspace_root, ws_cargo_toml) = find_workspace_root(manifest_dir, cargo_toml)?;
        let ws_config = ws_cargo_toml
            .workspace
            .and_then(|w| w.metadata)
            .and_then(|m| m.stylance);
        if let Some(mut ws_config) = ws_config {
            // Absolutize workspace config paths against the workspace root
            if let Some(p) = ws_config.hash_root_path.take() {
                ws_config.hash_root_path =
                    Some(relative_path(manifest_dir, &workspace_root.join(p))?);
            }
            if let Some(p) = ws_config.output_file.take() {
                ws_config.output_file = Some(relative_path(manifest_dir, &workspace_root.join(p))?);
            }
            if let Some(p) = ws_config.output_dir.take() {
                ws_config.output_dir = Some(relative_path(manifest_dir, &workspace_root.join(p))?);
            }
            config = config.merged_with_workspace(ws_config);
        }
        if config.hash_root_path.is_none() {
            config.hash_root_path = Some(relative_path(manifest_dir, &workspace_root)?);
        }
    }

    if config.extensions().iter().any(|e| e.is_empty()) {
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
    let hash_str = make_hash(hash_root, css_file, config.hash_len())?;
    let css_file_contents = fs::read_to_string(css_file)?;

    let fragments = parse::parse_css(&css_file_contents).map_err(|e| anyhow!("{e}"))?;

    let mut new_file = String::with_capacity(css_file_contents.len() * 2);
    let mut cursor = css_file_contents.as_str();

    for fragment in fragments {
        let (span, replace) = match fragment {
            CssFragment::Class(class) => (
                class,
                Cow::Owned(config.class_name_pattern().apply(class, &hash_str)),
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
    let hash_str = make_hash(hash_root, css_file, config.hash_len())?;

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
                hashed_name: config.class_name_pattern().apply(class, &hash_str),
            })
            .collect(),
    ))
}
