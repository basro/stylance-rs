use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr as _,
};

use anyhow::{bail, Context};
use serde::Deserialize;

use crate::class_name_pattern::ClassNamePattern;

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
 * Represents the stylance config of applying to a single crate.
 * Unlike PartialConfig, the paths in this struct should be interpreted
 * as relative to CWD instead of relative to a manifest dir.
 */
pub struct Config {
    pub manifest_dir: PathBuf,
    pub workspace_dir: Option<PathBuf>,
    pub output_file: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub extensions: Vec<String>,
    pub folders: Vec<PathBuf>,
    pub scss_prelude: Option<String>,
    pub hash_len: usize,
    pub class_name_pattern: ClassNamePattern,
    pub hash_root_path: PathBuf,
}

impl Config {
    pub fn load(manifest_dir: PathBuf) -> anyhow::Result<Self> {
        let cargo_toml_contents = fs::read_to_string(manifest_dir.join("Cargo.toml"))
            .context("Failed to read Cargo.toml")?;
        let mut cargo_toml: CargoToml = toml::from_str(&cargo_toml_contents)?;

        let config = cargo_toml
            .package
            .as_mut()
            .and_then(|p| p.metadata.as_mut())
            .and_then(|m| m.stylance.take())
            .unwrap_or_default();

        let workspace = if config.workspace {
            let (workspace_root, mut ws_cargo_toml) =
                find_workspace_root(&manifest_dir, cargo_toml)?;
            let ws_config = ws_cargo_toml
                .workspace
                .as_mut()
                .and_then(|w| w.metadata.as_mut())
                .and_then(|m| m.stylance.take())
                .unwrap_or_default();
            Some((workspace_root, ws_config))
        } else {
            None
        };

        Self::from_partials(manifest_dir, config, workspace)
    }

    fn from_partials(
        manifest_dir: PathBuf,
        config: PartialConfig,
        workspace: Option<(PathBuf, PartialConfig)>,
    ) -> anyhow::Result<Self> {
        let (workspace_dir, ws_config) = match workspace {
            Some((workspace_dir, mut ws_config)) => {
                // Absolutize workspace config paths against the workspace root
                ws_config.hash_root_path = Some(
                    ws_config
                        .hash_root_path
                        .map_or_else(|| workspace_dir.clone(), |p| workspace_dir.join(p)),
                );
                ws_config.output_file = ws_config.output_file.map(|p| workspace_dir.join(p));
                ws_config.output_dir = ws_config.output_dir.map(|p| workspace_dir.join(p));
                (Some(workspace_dir), ws_config)
            }
            None => (None, PartialConfig::default()),
        };

        let config = Self {
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
            workspace_dir,
            manifest_dir,
        };

        if config.extensions.iter().any(|e| e.is_empty()) {
            bail!("Stylance config extensions can't be empty strings");
        }

        Ok(config)
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
