use std::{
    borrow::Cow,
    collections::HashMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
};

pub use stylance_core::Config;
use stylance_core::ModifyCssResult;
use walkdir::WalkDir;

mod internal_prelude {
    pub use crate::errors::*;
    pub use color_eyre::eyre::bail;
    pub use tracing::*;
}
use internal_prelude::*;

pub mod errors {
    pub use color_eyre::eyre::Report as Error;
    pub use color_eyre::eyre::Result;
}

pub mod tracing {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;

    use crate::internal_prelude::*;

    pub fn install_tracing() -> Result<()> {
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_level(false)
            .without_time();
        let filter_layer = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new("info,stylance_cli=debug,stylance_core=debug"))?;

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .init();

        color_eyre::install()?;
        Ok(())
    }
}

pub fn run(manifest_dir: &Path, config: &Config) -> Result<()> {
    info!("Running stylance");

    let mut modified_css_files = Vec::new();

    for folder in &config.folders {
        for (entry, meta) in WalkDir::new(manifest_dir.join(folder))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|entry| entry.metadata().ok().map(|meta| (entry, meta)))
        {
            if meta.is_file() {
                let path_str = entry.path().to_string_lossy();
                if config.extensions.iter().any(|ext| path_str.ends_with(ext)) {
                    info!("Processing: {}", entry.path().display());
                    modified_css_files.push(stylance_core::load_and_modify_css(
                        manifest_dir,
                        entry.path(),
                        config,
                    )?);
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
