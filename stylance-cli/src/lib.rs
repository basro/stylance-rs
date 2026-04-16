use std::{
    borrow::Cow,
    collections::HashMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
};

use anyhow::bail;
pub use stylance_core::Config;
use stylance_core::ModifyCssResult;
use walkdir::WalkDir;

pub fn run(config: &Config) -> anyhow::Result<()> {
    println!("Running stylance");
    run_silent(config, |file_path| println!("{}", file_path.display()))
}

pub fn run_silent(
    config: &Config,
    mut file_visit_callback: impl FnMut(&Path),
) -> anyhow::Result<()> {
    let mut modified_css_files = Vec::new();

    for folder in config.folders.iter() {
        for (entry, meta) in WalkDir::new(config.manifest_dir.join(folder))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|entry| entry.metadata().ok().map(|meta| (entry, meta)))
        {
            if meta.is_file() {
                let path_str = entry.path().to_string_lossy();
                if config.extensions.iter().any(|ext| path_str.ends_with(ext)) {
                    file_visit_callback(entry.path());
                    modified_css_files
                        .push(stylance_core::load_and_modify_css(entry.path(), config)?);
                }
            }
        }
    }

    {
        // Verify that there are no hash collisions
        let mut map = HashMap::new();
        for file in modified_css_files.iter() {
            if let Some(previous_file) = map.insert(&file.hash, file) {
                bail!(
                    "The following files had a hash collision:\n{}\n{}\nConsider increasing the hash_len setting.",
                    file.path.to_string_lossy(),
                    previous_file.path.to_string_lossy()
                );
            }
        }
    }

    {
        // sort by (filename, path)
        fn key(a: &ModifyCssResult) -> (&std::ffi::OsStr, &String) {
            (
                a.path.file_name().expect("should be a file"),
                &a.normalized_path_str,
            )
        }
        modified_css_files.sort_unstable_by(|a, b| key(a).cmp(&key(b)));
    }

    if let Some(output_file) = &config.output_file {
        if let Some(parent) = output_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = BufWriter::new(File::create(output_file)?);

        if let Some(scss_prelude) = &config.scss_prelude {
            if output_file
                .extension()
                .filter(|ext| ext.to_string_lossy() == "scss")
                .is_some()
            {
                file.write_all(scss_prelude.as_bytes())?;
                file.write_all(b"\n\n")?;
            }
        }

        file.write_all(
            modified_css_files
                .iter()
                .map(|r| r.contents.as_ref())
                .collect::<Vec<_>>()
                .join("\n\n")
                .as_bytes(),
        )?;
    }

    if let Some(output_dir) = &config.output_dir {
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
        for modified_css in modified_css_files {
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

            if let Some(scss_prelude) = &config.scss_prelude {
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
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn test_symlinked_folder() {
        use super::*;
        use stylance_core::Config;
        use stylance_core::PartialConfig;

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

        let config = Config::from_partials(
            manifest_dir,
            PartialConfig {
                output_file: Some(base.join("out.css")),
                folders: Some(vec![std::path::PathBuf::from("./views/")]),
                ..Default::default()
            },
            None,
        )
        .unwrap();

        run_silent(&config, |_| {}).expect("run_silent should succeed with symlinked folder");

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
