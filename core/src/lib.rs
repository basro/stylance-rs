mod parse;

use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash as _, Hasher as _},
    path::Path,
};

use anyhow::anyhow;
use parse::{CssFragment, Global};

pub fn hash_string(input: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

pub struct Class {
    pub original_name: String,
    pub hashed_name: String,
}

fn make_hash(manifest_dir: &Path, css_file: &Path) -> anyhow::Result<String> {
    let manifest_dir = manifest_dir.canonicalize()?;
    let css_file = css_file.canonicalize()?;

    let relative_path_str = css_file.strip_prefix(manifest_dir)?.to_string_lossy();

    #[cfg(target_os = "windows")]
    let relative_path_str = relative_path_str.replace('\\', "/");

    println!("{}", relative_path_str);

    let hash = hash_string(&relative_path_str);
    let mut hash_str = format!("{hash:x}");
    hash_str.truncate(7);
    Ok(hash_str)
}

fn modify_class(class: &str, hash_str: &str) -> String {
    format!("{class}-{hash_str}")
}

pub fn load_and_modify_css(manifest_dir: &Path, css_file: &Path) -> anyhow::Result<String> {
    let hash_str = make_hash(manifest_dir, css_file)?;
    let css_file_contents = fs::read_to_string(css_file)?;

    let fragments = parse::parse_css(&css_file_contents).map_err(|e| anyhow!("{e}"))?;

    let mut new_file = String::with_capacity(css_file_contents.len() * 2);
    let mut cursor = css_file_contents.as_str();

    for fragment in fragments {
        let (span, replace) = match fragment {
            CssFragment::Class(class) => (class, Cow::Owned(modify_class(class, &hash_str))),
            CssFragment::Global(Global { inner, outer }) => (outer, Cow::Borrowed(inner)),
        };

        let (before, after) = cursor.split_at(span.as_ptr() as usize - cursor.as_ptr() as usize);
        cursor = &after[span.len()..];
        new_file.push_str(before);
        new_file.push_str(&replace);
    }

    new_file.push_str(cursor);

    Ok(new_file)
}

pub fn get_classes(manifest_dir: &Path, css_file: &Path) -> anyhow::Result<(String, Vec<Class>)> {
    let hash_str = make_hash(manifest_dir, css_file)?;

    let css_file_contents = fs::read_to_string(css_file)?;

    let mut classes = parse::parse_css(&css_file_contents)
        .map_err(|_| anyhow!("Failed to parse css file"))?
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
                hashed_name: modify_class(class, &hash_str),
            })
            .collect(),
    ))
}
