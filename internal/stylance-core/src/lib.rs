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
pub struct PartialConfig {
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

/**
 * Represents the stylance config of applying to a single crate
 * Paths should be already resolved to be independent of the manifest_dir
 */
pub struct Config {
    pub output_file: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub extensions: Vec<String>,
    pub folders: Vec<PathBuf>,
    pub scss_prelude: Option<String>,
    pub hash_len: usize,
    pub class_name_pattern: ClassNamePattern,
    pub hash_root_path: PathBuf,
    pub workspace: bool,
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
    stylance: Option<PartialConfig>,
}

#[derive(Deserialize)]
struct CargoTomlWorkspace {
    metadata: Option<CargoTomlWorkspaceMetadata>,
}

#[derive(Deserialize)]
struct CargoTomlWorkspaceMetadata {
    stylance: Option<PartialConfig>,
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

pub fn load_config(manifest_dir: &Path) -> anyhow::Result<Config> {
    let cargo_toml_contents =
        fs::read_to_string(manifest_dir.join("Cargo.toml")).context("Failed to read Cargo.toml")?;
    let mut cargo_toml: CargoToml = toml::from_str(&cargo_toml_contents)?;

    let config = cargo_toml
        .package
        .as_mut()
        .and_then(|p| p.metadata.as_mut())
        .and_then(|m| m.stylance.take())
        .unwrap_or_default();

    let ws_config = if config.workspace {
        let (workspace_root, mut ws_cargo_toml) = find_workspace_root(manifest_dir, cargo_toml)?;
        let mut ws_config = ws_cargo_toml
            .workspace
            .as_mut()
            .and_then(|w| w.metadata.as_mut())
            .and_then(|m| m.stylance.take())
            .unwrap_or_default();

        // Absolutize workspace config paths against the workspace root
        ws_config.hash_root_path = Some(
            ws_config
                .hash_root_path
                .map_or_else(|| workspace_root.clone(), |p| workspace_root.join(p)),
        );
        ws_config.output_file = ws_config.output_file.map(|p| workspace_root.join(p));
        ws_config.output_dir = ws_config.output_dir.map(|p| workspace_root.join(p));

        ws_config
    } else {
        PartialConfig::default()
    };

    // TODO: Resolve all paths

    let config = Config {
        output_file: config.output_file.or(ws_config.output_file),
        output_dir: config.output_dir.or(ws_config.output_dir),
        extensions: config
            .extensions
            .or(ws_config.extensions)
            .unwrap_or_else(default_extensions),
        folders: config
            .folders
            .or(ws_config.folders)
            .unwrap_or_else(default_folders),
        scss_prelude: config.scss_prelude.or(ws_config.scss_prelude),
        hash_len: config
            .hash_len
            .or(ws_config.hash_len)
            .unwrap_or(default_hash_len()),
        class_name_pattern: config
            .class_name_pattern
            .or(ws_config.class_name_pattern)
            .unwrap_or_default(),
        hash_root_path: config
            .hash_root_path
            .or(ws_config.hash_root_path)
            .unwrap_or_else(|| manifest_dir.to_path_buf()),
        workspace: config.workspace,
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

pub struct ModifyCssResult {
    pub path: PathBuf,
    pub normalized_path_str: String,
    pub hash: String,
    pub contents: String,
}

pub fn load_and_modify_css(css_file: &Path, config: &Config) -> anyhow::Result<ModifyCssResult> {
    let hash_str = make_hash(&config.hash_root_path, css_file, config.hash_len)?;
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
        normalized_path_str: normalized_relative_path(&config.hash_root_path, css_file)?,
        hash: hash_str,
        contents: new_file,
    })
}

pub fn get_classes(css_file: &Path, config: &Config) -> anyhow::Result<(String, Vec<Class>)> {
    let hash_str = make_hash(&config.hash_root_path, css_file, config.hash_len)?;

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
