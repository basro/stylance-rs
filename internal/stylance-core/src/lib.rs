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
        }
    }
}

#[derive(Deserialize)]
pub struct CargoToml {
    package: Option<CargoTomlPackage>,
}

#[derive(Deserialize)]
pub struct CargoTomlPackage {
    metadata: Option<CargoTomlPackageMetadata>,
}

#[derive(Deserialize)]
pub struct CargoTomlPackageMetadata {
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

pub fn load_config(manifest_dir: &Path) -> anyhow::Result<Config> {
    let cargo_toml_contents =
        fs::read_to_string(manifest_dir.join("Cargo.toml")).context("Failed to read Cargo.toml")?;
    let cargo_toml: CargoToml = toml::from_str(&cargo_toml_contents)?;

    let config = match cargo_toml.package {
        Some(CargoTomlPackage {
            metadata:
                Some(CargoTomlPackageMetadata {
                    stylance: Some(config),
                }),
        }) => config,
        _ => Config::default(),
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
