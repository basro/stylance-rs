use std::{fs::File, io::Write, path::PathBuf};

use clap::Parser;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    manifest_dir: PathBuf,

    #[arg(short, long)]
    output: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut modified_css_files = Vec::new();

    for (entry, meta) in WalkDir::new(&cli.manifest_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|entry| entry.metadata().ok().map(|meta| (entry, meta)))
    {
        if meta.is_file() {
            if let Some(extension) = entry.path().extension() {
                if extension == "scss" && entry.path().to_string_lossy().ends_with(".scss") {
                    println!("{}", entry.path().display());
                    modified_css_files
                        .push(core::load_and_modify_css(&cli.manifest_dir, entry.path())?);
                }
            }
        }
    }

    let mut file = File::create(cli.output)?;

    file.write_all(modified_css_files.join("\n\n").as_bytes())?;

    Ok(())
}
