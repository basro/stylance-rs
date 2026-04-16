mod class_name_pattern;
mod config;
mod parse;

use std::{
    borrow::Cow,
    fs,
    hash::{Hash as _, Hasher as _},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context};
use parse::{CssFragment, Global};
use siphasher::sip::SipHasher13;

pub use crate::config::{Config, PartialConfig};

pub fn hash_string(input: &str) -> u64 {
    let mut hasher = SipHasher13::new();
    input.hash(&mut hasher);
    hasher.finish()
}

pub struct Class {
    pub original_name: String,
    pub hashed_name: String,
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
