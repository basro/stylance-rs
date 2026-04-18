mod class_name_pattern;
mod config;
mod parse;
pub mod path_utils;

use std::{
    borrow::Cow,
    fs,
    hash::{Hash as _, Hasher as _},
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use parse::{CssFragment, Global};
use siphasher::sip::SipHasher13;

pub use crate::config::{Config, PartialConfig};
use crate::path_utils::{diff_normalized_paths, normalize};

pub fn hash_path(input: &Path) -> u64 {
    let normalized_separators = input
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");

    let mut hasher = SipHasher13::new();
    normalized_separators.hash(&mut hasher);
    hasher.finish()
}

pub struct Class {
    pub original_name: String,
    pub hashed_name: String,
}

fn make_hash(relative_path: &Path, hash_len: usize) -> anyhow::Result<String> {
    let hash = hash_path(relative_path);
    let mut hash_str = format!("{hash:x}");
    hash_str.truncate(hash_len);
    Ok(hash_str)
}

pub struct ModifyCssResult {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub hash: String,
    pub contents: String,
}

pub fn load_and_modify_css(css_file: &Path, config: &Config) -> anyhow::Result<ModifyCssResult> {
    let css_file = normalize(css_file)?;
    let hash_root = normalize(&config.hash_root_path)?;
    let relative_path = diff_normalized_paths(&css_file, &hash_root)?;
    let hash_str = make_hash(&relative_path, config.hash_len)?;

    let css_file_contents = fs::read_to_string(&css_file)?;

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
        path: css_file,
        relative_path,
        hash: hash_str,
        contents: new_file,
    })
}

pub fn get_classes(css_file: &Path, config: &Config) -> anyhow::Result<(String, Vec<Class>)> {
    let css_file = normalize(css_file)?;
    let hash_root = normalize(&config.hash_root_path)?;
    let relative_path = diff_normalized_paths(&css_file, &hash_root)?;
    let hash_str = make_hash(&relative_path, config.hash_len)?;

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
