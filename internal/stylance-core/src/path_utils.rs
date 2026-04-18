use std::{
    io,
    path::{Component, Path, PathBuf},
};

use anyhow::Context;

// This was taken from the crate path-clean 1.0.1
// Added code verbatim to avoid introducing an extra dependency

/// Normalizes a path. It performs the following, lexically:
/// 1. Reduce multiple slashes to a single slash.
/// 2. Eliminate `.` path name elements (the current directory).
/// 3. Eliminate `..` path name elements (the parent directory) and the non-`.` non-`..`, element that precedes them.
/// 4. Eliminate `..` elements that begin a rooted path, that is, replace `/..` by `/` at the beginning of a path.
/// 5. Leave intact `..` elements that begin a non-rooted path.
///
/// If the result of this process is an empty string, return the string `"."`, representing the current directory.
pub fn clean<P>(path: P) -> PathBuf
where
    P: AsRef<Path>,
{
    let mut out = Vec::new();

    for comp in path.as_ref().components() {
        match comp {
            Component::CurDir => (),
            Component::ParentDir => match out.last() {
                Some(Component::RootDir) => (),
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                None
                | Some(Component::CurDir)
                | Some(Component::ParentDir)
                | Some(Component::Prefix(_)) => out.push(comp),
            },
            comp => out.push(comp),
        }
    }

    if !out.is_empty() {
        out.iter().collect()
    } else {
        PathBuf::from(".")
    }
}

/// If the path is relative it joins it with CWD to make it absolute and then cleans it.
pub fn normalize(path: &Path) -> io::Result<PathBuf> {
    Ok(if path.is_absolute() {
        clean(path)
    } else {
        clean(std::env::current_dir()?.join(path))
    })
}

pub fn normalized_relative_path(base: &Path, subpath: &Path) -> anyhow::Result<String> {
    let base = normalize(base)?;
    let subpath = normalize(subpath)?;

    let relative_path_str: String = subpath
        .strip_prefix(base)
        .context("css file should be inside the hash root path")?
        .to_string_lossy()
        .into();

    #[cfg(target_os = "windows")]
    let relative_path_str = relative_path_str.replace('\\', "/");

    Ok(relative_path_str)
}
