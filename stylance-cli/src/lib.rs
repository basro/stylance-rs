use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::bail;
pub use stylance_core::Config;
pub use stylance_core::ModifyCssResult;
use walkdir::WalkDir;

pub fn run(manifest_dir: &Path, config: &Config) -> anyhow::Result<()> {
    println!("Running stylance");
    run_silent(manifest_dir, config, |file_path| {
        println!("{}", file_path.display())
    })
}

pub fn run_silent(
    manifest_dir: &Path,
    config: &Config,
    file_visit_callback: impl FnMut(&Path),
) -> anyhow::Result<()> {
    let results = build_crate(manifest_dir, config, file_visit_callback)?;
    write_output(config, &results)
}

/// Build a single crate: discover CSS files, transform them, check for hash
/// collisions, and return the sorted results without writing anything to disk.
pub fn build_crate(
    manifest_dir: &Path,
    config: &Config,
    mut file_visit_callback: impl FnMut(&Path),
) -> anyhow::Result<Vec<ModifyCssResult>> {
    let hash_root = stylance_core::resolve_hash_root(manifest_dir, config);
    let mut modified_css_files = Vec::new();

    for folder in config.folders() {
        for (entry, meta) in WalkDir::new(manifest_dir.join(folder))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|entry| entry.metadata().ok().map(|meta| (entry, meta)))
        {
            if meta.is_file() {
                let path_str = entry.path().to_string_lossy();
                if config
                    .extensions()
                    .iter()
                    .any(|ext| path_str.ends_with(ext))
                {
                    file_visit_callback(entry.path());
                    modified_css_files.push(stylance_core::load_and_modify_css(
                        &hash_root,
                        entry.path(),
                        config,
                    )?);
                }
            }
        }
    }

    check_hash_collisions(&modified_css_files)?;
    sort_results(&mut modified_css_files);

    Ok(modified_css_files)
}

fn check_hash_collisions(files: &[ModifyCssResult]) -> anyhow::Result<()> {
    let mut map = HashMap::new();
    for file in files.iter() {
        if let Some(previous_file) = map.insert(&file.hash, file) {
            bail!(
                "The following files had a hash collision:\n{}\n{}\nConsider increasing the hash_len setting.",
                file.path.to_string_lossy(),
                previous_file.path.to_string_lossy()
            );
        }
    }
    Ok(())
}

fn sort_results(files: &mut [ModifyCssResult]) {
    fn key(a: &ModifyCssResult) -> (&std::ffi::OsStr, &String) {
        (
            a.path.file_name().expect("should be a file"),
            &a.normalized_path_str,
        )
    }
    files.sort_unstable_by(|a, b| key(a).cmp(&key(b)));
}

/// Write build results to disk for a single crate's config.
pub fn write_output(config: &Config, results: &[ModifyCssResult]) -> anyhow::Result<()> {
    if let Some(output_file) = &config.output_file {
        write_output_file(output_file, config.scss_prelude.as_deref(), &[results])?;
    }

    if let Some(output_dir) = &config.output_dir {
        write_output_dir(output_dir, config.scss_prelude.as_deref(), results)?;
    }

    Ok(())
}

/// Build the content string for a concatenated output file without writing it.
pub fn build_output_file_content(
    output_file: &Path,
    scss_prelude: Option<&str>,
    crate_results: &[&[ModifyCssResult]],
) -> String {
    let mut content = String::new();

    if let Some(scss_prelude) = scss_prelude {
        if output_file
            .extension()
            .filter(|ext| ext.to_string_lossy() == "scss")
            .is_some()
        {
            content.push_str(scss_prelude);
            content.push_str("\n\n");
        }
    }

    let all_contents: Vec<&str> = crate_results
        .iter()
        .flat_map(|results| results.iter().map(|r| r.contents.as_ref()))
        .collect();

    content.push_str(&all_contents.join("\n\n"));
    content
}

/// Write one or more crates' results into a single concatenated CSS file.
/// Each slice in `crate_results` is one crate's sorted output; they are
/// concatenated in the order given.
pub fn write_output_file(
    output_file: &Path,
    scss_prelude: Option<&str>,
    crate_results: &[&[ModifyCssResult]],
) -> anyhow::Result<()> {
    if let Some(parent) = output_file.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = build_output_file_content(output_file, scss_prelude, crate_results);
    let mut file = BufWriter::new(File::create(output_file)?);
    file.write_all(content.as_bytes())?;

    Ok(())
}

pub fn write_output_dir(
    output_dir: &Path,
    scss_prelude: Option<&str>,
    results: &[ModifyCssResult],
) -> anyhow::Result<()> {
    let output_dir = output_dir.join("stylance");
    fs::create_dir_all(&output_dir)?;

    let entries = fs::read_dir(&output_dir)?;

    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;

        if file_type.is_file() {
            fs::remove_file(entry.path())?;
        }
    }

    let mut new_files = Vec::new();
    for modified_css in results {
        let extension = modified_css
            .path
            .extension()
            .map(|e| e.to_string_lossy())
            .filter(|e| e == "css")
            .unwrap_or(Cow::from("scss"));

        let new_file_name = format!(
            "{}-{}.{extension}",
            modified_css
                .path
                .file_stem()
                .expect("This path should be a file")
                .to_string_lossy(),
            modified_css.hash
        );

        new_files.push(new_file_name.clone());

        let file_path = output_dir.join(new_file_name);
        let mut file = BufWriter::new(File::create(file_path)?);

        if let Some(scss_prelude) = scss_prelude {
            if extension == "scss" {
                file.write_all(scss_prelude.as_bytes())?;
                file.write_all(b"\n\n")?;
            }
        }

        file.write_all(modified_css.contents.as_bytes())?;
    }

    let mut file = File::create(output_dir.join("_index.scss"))?;
    file.write_all(
        new_files
            .iter()
            .map(|f| format!("@use \"{f}\";\n"))
            .collect::<Vec<_>>()
            .join("")
            .as_bytes(),
    )?;

    Ok(())
}

/// Stateful per-crate builder that caches per-file results.
/// On incremental rebuilds only the changed files are re-processed.
pub struct CrateBuilder {
    manifest_dir: PathBuf,
    config: Config,
    hash_root: PathBuf,
    files: HashMap<PathBuf, ModifyCssResult>,
}

impl CrateBuilder {
    pub fn new(manifest_dir: PathBuf, config: Config) -> Self {
        let hash_root = stylance_core::resolve_hash_root(&manifest_dir, &config);
        Self {
            manifest_dir,
            config,
            hash_root,
            files: HashMap::new(),
        }
    }

    /// Create a builder pre-populated with existing build results.
    pub fn from_results(
        manifest_dir: PathBuf,
        config: Config,
        results: Vec<ModifyCssResult>,
    ) -> Self {
        let hash_root = stylance_core::resolve_hash_root(&manifest_dir, &config);
        let files = results.into_iter().map(|r| (r.path.clone(), r)).collect();
        Self {
            manifest_dir,
            config,
            hash_root,
            files,
        }
    }

    /// Full rebuild: clear cache and re-process all CSS files in the crate.
    pub fn build_all(&mut self) -> anyhow::Result<()> {
        self.files.clear();

        for folder in self.config.folders() {
            for (entry, meta) in WalkDir::new(self.manifest_dir.join(folder))
                .into_iter()
                .filter_map(|e| e.ok())
                .filter_map(|entry| entry.metadata().ok().map(|meta| (entry, meta)))
            {
                if meta.is_file() {
                    let path_str = entry.path().to_string_lossy();
                    if self
                        .config
                        .extensions()
                        .iter()
                        .any(|ext| path_str.ends_with(ext))
                    {
                        let result = stylance_core::load_and_modify_css(
                            &self.hash_root,
                            entry.path(),
                            &self.config,
                        )?;
                        self.files.insert(entry.path().to_path_buf(), result);
                    }
                }
            }
        }

        Ok(())
    }

    /// Incremental rebuild: only re-process the files that changed.
    pub fn rebuild_changed(&mut self, changed: &HashSet<PathBuf>) -> anyhow::Result<()> {
        for path in changed {
            if path.exists() {
                let result =
                    stylance_core::load_and_modify_css(&self.hash_root, path, &self.config)?;
                self.files.insert(path.clone(), result);
            } else {
                self.files.remove(path);
            }
        }
        Ok(())
    }

    /// Swap in a new config and trigger a full rebuild.
    pub fn update_config(&mut self, config: Config) -> anyhow::Result<()> {
        self.hash_root = stylance_core::resolve_hash_root(&self.manifest_dir, &config);
        self.config = config;
        self.build_all()
    }

    /// Return the cached results after collision-checking and sorting.
    pub fn output(&self) -> anyhow::Result<Vec<ModifyCssResult>> {
        let mut results: Vec<ModifyCssResult> = self.files.values().cloned().collect();
        check_hash_collisions(&results)?;
        sort_results(&mut results);
        Ok(results)
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn manifest_dir(&self) -> &Path {
        &self.manifest_dir
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn test_symlinked_folder() {
        use super::*;
        use stylance_core::Config;

        use std::os::unix::fs::symlink;

        let base = std::env::temp_dir().join(format!("stylance_test_{}", std::process::id()));
        let manifest_dir = base.join("my_app");
        let external_dir = base.join("external");

        fs::create_dir_all(&manifest_dir).unwrap();
        fs::create_dir_all(&external_dir).unwrap();

        fs::write(
            external_dir.join("style.module.css"),
            ".myClass { color: red; }",
        )
        .unwrap();

        // Create symlink: my_app/views -> ../external
        symlink(&external_dir, manifest_dir.join("views")).unwrap();

        let config = Config {
            output_file: Some(base.join("out.css")),
            folders: Some(vec![std::path::PathBuf::from("./views/")]),
            ..Config::default()
        };

        run_silent(&manifest_dir, &config, |_| {})
            .expect("run_silent should succeed with symlinked folder");

        let output = fs::read_to_string(base.join("out.css")).expect("output file should exist");

        fs::remove_dir_all(&base).unwrap();

        // Class name should be scoped: .myClass-<hash>
        assert!(
            output.contains(".myClass-"),
            "output should contain scoped class name, got: {output}"
        );
        // CSS body should be preserved
        assert!(
            output.contains("color: red"),
            "output should contain original CSS body, got: {output}"
        );
    }
}
