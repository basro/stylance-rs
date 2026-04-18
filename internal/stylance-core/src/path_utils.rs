use std::{
    io,
    path::{Component, Path, PathBuf},
};

use anyhow::bail;

/// Cleans a path. It performs the following, lexically:
/// 1. Reduce multiple slashes to a single slash.
/// 2. Eliminate `.` path name elements (the current directory).
/// 3. Eliminate `..` path name elements (the parent directory) and the non-`.` non-`..`, element that precedes them.
/// 4. Eliminate `..` elements that begin a rooted path, that is, replace `/..` by `/` at the beginning of a path.
/// 5. Leave intact `..` elements that begin a non-rooted path.
///
/// If the result of this process is an empty string, return the string `"."`, representing the current directory.
///
/// This was taken from the crate path-clean 1.0.1
/// Code was copied in order to avoid introducing small dependencies
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
pub fn normalize<P>(path: P) -> io::Result<PathBuf>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    Ok(if path.is_absolute() {
        clean(path)
    } else {
        clean(std::env::current_dir()?.join(path))
    })
}

/**
Computes the relative path between from_path and to_path.

Joining the resulting path with from_path will result in a path that points to the same
place as to_path.

Errors if the paths have different prefixes. (for example they point to different hard
drives in windows)

## Panics
This function can panic if the paths have not been
normalized (they must be absolute and contain no `.` or `..` components)
*/
pub fn diff_normalized_paths<P, B>(to_path: P, from_path: B) -> anyhow::Result<PathBuf>
where
    P: AsRef<Path>,
    B: AsRef<Path>,
{
    let to_path = to_path.as_ref();
    let from_path = from_path.as_ref();

    assert!(to_path.is_absolute() && from_path.is_absolute());

    let mut ita = to_path.components().peekable();
    let mut itb = from_path.components().peekable();

    // Skip the common prefix between the two paths
    while let (Some(a), Some(b)) = (ita.peek(), itb.peek()) {
        match (a, b) {
            (Component::Prefix(pa), Component::Prefix(pb)) if pa != pb => {
                bail!("Path prefix doesn't match")
            }
            _ if a == b => {
                ita.next();
                itb.next();
            }
            _ => break,
        }
    }

    let mut result = Vec::new();

    // For each remaining component in base, go up one level
    for comp in itb {
        assert!(matches!(comp, Component::Normal(_)));
        result.push(Component::ParentDir);
    }

    // Then descend into the remaining part of path
    for comp in ita {
        result.push(comp);
    }

    if result.is_empty() {
        result.push(Component::CurDir);
    }

    Ok(result.iter().collect())
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use crate::path_utils::{diff_normalized_paths, normalize};

    #[test]
    fn test_diff_normalized_paths() {
        assert_eq!(
            diff_normalized_paths(
                normalize("/some/path").unwrap(),
                normalize("/some/foo/baz/path").unwrap()
            )
            .unwrap(),
            PathBuf::from("../../../path")
        );

        assert_eq!(
            diff_normalized_paths(
                normalize("/some/foo/baz/path").unwrap(),
                normalize("/some/path").unwrap(),
            )
            .unwrap(),
            PathBuf::from("../foo/baz/path")
        );

        assert_eq!(
            diff_normalized_paths(
                normalize("some/path").unwrap(),
                normalize("some/foo/baz/path").unwrap()
            )
            .unwrap(),
            PathBuf::from("../../../path")
        );

        assert_eq!(
            diff_normalized_paths(
                normalize("some/foo/baz/path").unwrap(),
                normalize("some/path").unwrap(),
            )
            .unwrap(),
            PathBuf::from("../foo/baz/path")
        );
    }
}
