use core::load_config;
use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread::{self, sleep},
    time::Duration,
};

use anyhow::anyhow;
use clap::Parser;
use notify::{Event, RecursiveMode, Watcher};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(arg_required_else_help = true)]
struct Cli {
    manifest_dir: PathBuf,

    #[arg(short, long)]
    output: Option<PathBuf>,

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
    output_file: PathBuf,
    extensions: Vec<String>,
    folders: Vec<PathBuf>,
}

fn make_run_config(cli: &Cli) -> anyhow::Result<RunConfig> {
    let config = load_config(&cli.manifest_dir)?;

    let output_file = cli
        .output
        .clone()
        .or_else(|| config.output.as_ref().map(|p| cli.manifest_dir.join(p)))
        .ok_or_else(|| anyhow!("Output not specified"))?;

    Ok(RunConfig {
        manifest_dir: cli.manifest_dir.clone(),
        output_file,
        extensions: config.extensions,
        folders: config.folders,
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
                    modified_css_files.push(core::load_and_modify_css(
                        &config.manifest_dir,
                        entry.path(),
                    )?);
                }
            }
        }
    }

    let mut file = File::create(&config.output_file)?;
    file.write_all(modified_css_files.join("\n\n").as_bytes())?;
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
                    println!("{e}");
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
                            println!("{e}");
                        }
                    }
                }
            }
        }
    }
}
