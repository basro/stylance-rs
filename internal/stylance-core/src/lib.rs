mod class_name_pattern;
mod parse;

pub use class_name_pattern::{ClassNamePattern, Fragment};

use std::{
    borrow::Cow,
    fs,
    hash::{Hash as _, Hasher as _},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context};
use parse::{CssFragment, Global};
use serde::Deserialize;
use siphasher::sip::SipHasher13;
use winnow::{
    error::{ContextError, ParseError},
    Parser,
};

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

fn normalized_relative_path(base: &Path, subpath: &Path) -> anyhow::Result<String> {
    let base = base.canonicalize()?;
    let subpath = subpath.canonicalize()?;

    let relative_path_str: String = subpath
        .strip_prefix(base)
        .context("css file should be inside manifest_dir")?
        .to_string_lossy()
        .into();

    #[cfg(target_os = "windows")]
    let relative_path_str = relative_path_str.replace('\\', "/");

    Ok(relative_path_str)
}

fn make_hash(manifest_dir: &Path, css_file: &Path, hash_len: usize) -> anyhow::Result<String> {
    let relative_path_str = normalized_relative_path(manifest_dir, css_file)?;

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

pub fn load_and_modify_css(
    manifest_dir: &Path,
    css_file: &Path,
    config: &Config,
) -> anyhow::Result<ModifyCssResult> {
    let hash = make_hash(manifest_dir, css_file, config.hash_len)?;
    let css_file_contents = fs::read_to_string(css_file)?;

    let contents = transform_css(&css_file_contents, &config.class_name_pattern, &hash)
        .map_err(|e| anyhow!("{e}"))?;

    Ok(ModifyCssResult {
        path: css_file.to_owned(),
        normalized_path_str: normalized_relative_path(manifest_dir, css_file)?,
        hash,
        contents,
    })
}

/// Parses and rewrites CSS class selectors
pub fn transform_css<'a>(
    css: &'a str,
    class_name_pattern: &ClassNamePattern,
    hash: &str,
) -> Result<String, ParseError<&'a str, ContextError>> {
    let fragments = parse::parse_css(&css)?;

    let mut new_css = String::with_capacity(css.len() * 2);
    let mut cursor = css;

    for fragment in fragments {
        let (span, replace) = match fragment {
            CssFragment::Class(class) => (class, Cow::Owned(class_name_pattern.apply(class, hash))),
            CssFragment::Global(Global { inner, outer }) => (outer, Cow::Borrowed(inner)),
        };

        let (before, after) = cursor.split_at(span.as_ptr() as usize - cursor.as_ptr() as usize);
        cursor = &after[span.len()..];
        new_css.push_str(before);
        new_css.push_str(&replace);
    }

    new_css.push_str(cursor);
    Ok(new_css)
}

pub fn get_classes(
    manifest_dir: &Path,
    css_file: &Path,
    config: &Config,
) -> anyhow::Result<(String, Vec<Class>)> {
    let hash_str = make_hash(manifest_dir, css_file, config.hash_len)?;

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

pub fn get_class_mappings<'a>(
    css: &'a str,
    class_name_pattern: &ClassNamePattern,
    hash: &str,
    include_global: bool,
) -> Result<Vec<(&'a str, Cow<'a, str>)>, ParseError<&'a str, ContextError>> {
    let fragments = parse::parse_css(&css)?;
    let mut result = Vec::new();

    if include_global {
        for c in fragments {
            match c {
                CssFragment::Class(class) => {
                    result.push((class, Cow::Owned(class_name_pattern.apply(class, hash))));
                }
                CssFragment::Global(global) => {
                    let global_classes = resolve_global_inner_classes(global)?;
                    result.extend(
                        global_classes
                            .into_iter()
                            .map(|class| (class, Cow::Borrowed(class))),
                    );
                }
            }
        }
    } else {
        for c in fragments {
            if let CssFragment::Class(class) = c {
                result.push((class, Cow::Owned(class_name_pattern.apply(class, hash))));
            }
        }
    }
    result.sort_by_key(|e| e.0);
    result.dedup_by_key(|e| e.0);
    Ok(result)
}

fn resolve_global_inner_classes<'a>(
    global: Global<'a>,
) -> Result<Vec<&'a str>, ParseError<&'a str, ContextError>> {
    let mut input = global.inner;
    let fragments = parse::selector.parse(&mut input)?;
    let mut result = Vec::new();
    for c in fragments {
        match c {
            CssFragment::Class(class) => result.push(class),
            CssFragment::Global(_) => {
                unreachable!("Top level parser should have already errored if globals are nested")
            }
        }
    }
    Ok(result)
}

#[test]
fn test_get_class_mappings() {
    let css = r#".foo.bar {
        background-color: red;
        :global(.baz) {
            color: blue;
        }
        :global(.bag .biz) {
            color: blue;
        }
        .zig {

        }
        .bong {}
        .zig {
            color: blue;
        }
    }"#;
    let pattern = ClassNamePattern::default();
    let hash = "abc1234";
    let mappings = get_class_mappings(css, &pattern, hash, true).unwrap();
    let expected = vec![
        ("bag", "bag"),
        ("bar", "bar-abc1234"),
        ("baz", "baz"),
        ("biz", "biz"),
        ("bong", "bong-abc1234"),
        ("foo", "foo-abc1234"),
        ("zig", "zig-abc1234"),
    ];
    if mappings.len() != expected.len() {
        panic!(
            "Expected {} mappings, got {}",
            expected.len(),
            mappings.len()
        );
    }
    for (i, (original, hashed)) in mappings.iter().enumerate() {
        assert_eq!(expected[i].0, *original);
        assert_eq!(expected[i].1, *hashed);
    }

    let mappings = get_class_mappings(css, &pattern, hash, false).unwrap();
    let expected = vec![
        ("bar", "bar-abc1234"),
        ("bong", "bong-abc1234"),
        ("foo", "foo-abc1234"),
        ("zig", "zig-abc1234"),
    ];
    if mappings.len() != expected.len() {
        panic!(
            "Expected {} mappings, got {}",
            expected.len(),
            mappings.len()
        );
    }
    for (i, (original, hashed)) in mappings.iter().enumerate() {
        assert_eq!(expected[i].0, *original);
        assert_eq!(expected[i].1, *hashed);
    }
}

#[test]
fn test_parser_error_on_nested_globals() {
    let css = r#".foo :global(.bar .baz) {
        color: blue;
    }"#;
    let result = parse::parse_css(css);
    assert!(result.is_ok());
    let css = r#".foo :global(.bar :global(.baz)) {
        color: blue;
    }"#;
    let result = parse::parse_css(css);
    assert!(result.is_err());
}

#[test]
#[should_panic]
fn test_resolve_global_inner_classes_nested() {
    let global = Global {
        inner: ".foo :global(.bar)".into(),
        outer: ":global(.foo :global(.bar))".into(),
    };
    let _ = resolve_global_inner_classes(global);
}
