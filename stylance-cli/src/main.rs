use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread::{self, sleep},
    time::Duration,
};
use stylance_core::load_config;

use clap::Parser;
use notify::{Event, RecursiveMode, Watcher};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(author, version, about, long_about = None, arg_required_else_help = true)]
struct Cli {
    /// The path where your crate's Cargo toml is located
    manifest_dir: PathBuf,

    /// Generate a file with all css modules concatenated
    #[arg(long)]
    output_file: Option<PathBuf>,

    /// Generate a "stylance" directory in this path with all css modules inside
    #[arg(long)]
    output_dir: Option<PathBuf>,

    /// The folders in your crate where stylance will look for css modules
    ///
    /// The paths are relative to the manifest_dir and must not land outside of manifest_dir.
    #[arg(short, long, num_args(1))]
    folder: Vec<PathBuf>,

    /// Watch the fylesystem for changes to the css module files
    #[arg(short, long)]
    watch: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let run_config = make_run_config(&cli)?;

    run(&run_config)?;

    if cli.watch {
        watch(&cli, run_config)?;
    }

    Ok(())
}

struct RunConfig {
    manifest_dir: PathBuf,
    output_file: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    extensions: Vec<String>,
    folders: Vec<PathBuf>,
}

fn make_run_config(cli: &Cli) -> anyhow::Result<RunConfig> {
    let config = load_config(&cli.manifest_dir)?;

    let output_file = cli.output_file.clone().or_else(|| {
        config
            .output_file
            .as_ref()
            .map(|p| cli.manifest_dir.join(p))
    });

    let output_dir = cli
        .output_dir
        .clone()
        .or_else(|| config.output_dir.as_ref().map(|p| cli.manifest_dir.join(p)));

    let folders = if cli.folder.is_empty() {
        config.folders
    } else {
        cli.folder.clone()
    };

    Ok(RunConfig {
        manifest_dir: cli.manifest_dir.clone(),
        output_file,
        output_dir,
        extensions: config.extensions,
        folders,
    })
}

fn run(config: &RunConfig) -> anyhow::Result<()> {
    println!("Running stylance");

    let mut modified_css_files = Vec::new();

    for folder in &config.folders {
        for (entry, meta) in WalkDir::new(&config.manifest_dir.join(folder))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|entry| entry.metadata().ok().map(|meta| (entry, meta)))
        {
            if meta.is_file() {
                let path_str = entry.path().to_string_lossy();
                if config.extensions.iter().any(|ext| path_str.ends_with(ext)) {
                    println!("{}", entry.path().display());
                    modified_css_files.push(stylance_core::load_and_modify_css(
                        &config.manifest_dir,
                        entry.path(),
                    )?);
                }
            }
        }
    }

    if let Some(output_file) = &config.output_file {
        if let Some(parent) = output_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = File::create(output_file)?;
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
            let new_file_name = format!(
                "{}-{}.scss",
                modified_css
                    .path
                    .file_stem()
                    .expect("This path should be a file")
                    .to_string_lossy(),
                modified_css.hash
            );

            new_files.push(new_file_name.clone());

            let file_path = output_dir.join(new_file_name);
            let mut file = File::create(file_path)?;
            file.write_all(modified_css.contents.as_bytes())?;
        }

        let mut file = File::create(output_dir.join("_all.scss"))?;
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

fn watch(cli: &Cli, run_config: RunConfig) -> anyhow::Result<()> {
    let (run_event_tx, run_event_rx) = mpsc::sync_channel(0);

    let run_config = Arc::new(Mutex::new(Arc::new(run_config)));

    thread::spawn({
        let run_config = run_config.clone();
        move || {
            while run_event_rx.recv().is_ok() {
                let run_config = run_config.lock().unwrap().clone();
                if let Err(e) = run(&run_config) {
                    eprintln!("{e}");
                }
                sleep(Duration::from_millis(100));
            }
        }
    });

    loop {
        let current_run_config = run_config.lock().unwrap().clone();

        let cargo_toml_path = cli.manifest_dir.join("Cargo.toml").canonicalize()?;

        let (watch_event_tx, watch_event_rx) = mpsc::channel();

        let mut watcher = notify::recommended_watcher(watch_event_tx)?;

        for folder in &current_run_config.folders {
            watcher.watch(&cli.manifest_dir.join(folder), RecursiveMode::Recursive)?;
        }

        watcher.watch(&cargo_toml_path, RecursiveMode::NonRecursive)?;

        'watch_events: while let Ok(Ok(Event { paths, .. })) = watch_event_rx.recv() {
            for path in paths {
                let str_path = path.to_string_lossy();
                if current_run_config
                    .extensions
                    .iter()
                    .any(|ext| str_path.ends_with(ext))
                {
                    let _ = run_event_tx.try_send(());
                    break;
                }

                if str_path.ends_with("Cargo.toml")
                    && path
                        .canonicalize()
                        .ok()
                        .filter(|p| *p == cargo_toml_path)
                        .is_some()
                {
                    match make_run_config(cli) {
                        Ok(new_config) => {
                            *run_config.lock().unwrap() = Arc::new(new_config);
                            break 'watch_events;
                        }
                        Err(e) => {
                            eprintln!("{e}");
                        }
                    }
                }
            }
        }
    }
}
