use anyhow::bail;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::Duration,
};
use stylance_cli::run;
use stylance_core::Config;

use clap::Parser;
use notify::{Event, RecursiveMode, Watcher};
use tokio::{
    sync::mpsc,
    task::{spawn_blocking, JoinSet},
    time::{sleep, Instant, Sleep},
};
use tokio_stream::{Stream, StreamExt};

#[derive(Parser)]
#[command(author, version, about, long_about = None, arg_required_else_help = true)]
struct Cli {
    /// The path(s) where your crate's Cargo toml is located.
    /// Multiple paths can be specified to process several crates at once.
    #[arg(required = true)]
    manifest_dirs: Vec<PathBuf>,

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

fn check_output_collisions(all_configs: &[Config]) -> anyhow::Result<()> {
    let mut seen_files: HashMap<PathBuf, &Path> = HashMap::new();
    let mut seen_dirs: HashMap<PathBuf, &Path> = HashMap::new();

    for config in all_configs {
        if let Some(output_file) = &config.output_file {
            if let Some(prev) = seen_files.insert(output_file.clone(), &config.manifest_dir) {
                bail!(
                    "Multiple crates share the same output_file: {}\n  - {}\n  - {}",
                    output_file.display(),
                    prev.display(),
                    config.manifest_dir.display(),
                );
            }
        }
        if let Some(output_dir) = &config.output_dir {
            if let Some(prev) = seen_dirs.insert(output_dir.clone(), &config.manifest_dir) {
                bail!(
                    "Multiple crates share the same output_dir: {}\n  - {}\n  - {}",
                    output_dir.display(),
                    prev.display(),
                    config.manifest_dir.display(),
                );
            }
        }
    }

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut all_configs = Vec::new();
    for manifest_dir in &cli.manifest_dirs {
        let params = load_config(&cli, manifest_dir).await?;
        all_configs.push(params);
    }

    check_output_collisions(&all_configs)?;

    for config in &all_configs {
        run(config)?;
    }

    if cli.watch {
        let cli = Arc::new(cli);

        // Spawn one independent watch task per manifest dir, as each crate
        // has its own config, folders, and output.
        let mut set = JoinSet::new();
        for config in all_configs {
            let cli = cli.clone();
            set.spawn(watch_single(cli, config));
        }

        // If any watcher ends (only happens on error), abort the rest and exit.
        if let Some(result) = set.join_next().await {
            set.abort_all();
            result??;
        }
    }

    Ok(())
}

async fn load_config(cli: &Cli, manifest_dir: &Path) -> anyhow::Result<Config> {
    let mut config = spawn_blocking({
        let manifest_dir = manifest_dir.to_path_buf();
        move || Config::load(manifest_dir)
    })
    .await??;

    config.output_file = cli.output_file.clone().or(config.output_file);
    config.output_dir = cli.output_dir.clone().or(config.output_dir);

    if !cli.folder.is_empty() {
        config.folders = cli.folder.iter().map(|p| manifest_dir.join(p)).collect();
    }

    Ok(config)
}

fn watch_files(paths: &[PathBuf]) -> anyhow::Result<mpsc::UnboundedReceiver<()>> {
    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher({
        let events_tx = events_tx.clone();
        move |e: notify::Result<Event>| {
            let Ok(e) = e else {
                return;
            };

            // Ignore access events
            if matches!(e.kind, notify::EventKind::Access(_)) {
                return;
            }

            let _ = events_tx.send(());
        }
    })?;

    for path in paths {
        watcher.watch(path, RecursiveMode::NonRecursive)?;
    }

    tokio::spawn(async move {
        events_tx.closed().await;
        drop(watcher);
    });

    Ok(events_rx)
}

fn watch_folders(paths: &[PathBuf]) -> anyhow::Result<mpsc::UnboundedReceiver<PathBuf>> {
    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher({
        let events_tx = events_tx.clone();
        move |e: notify::Result<Event>| {
            let Ok(e) = e else {
                return;
            };

            // Ignore access events
            if matches!(e.kind, notify::EventKind::Access(_)) {
                return;
            }

            for path in e.paths {
                if events_tx.send(path).is_err() {
                    break;
                }
            }
        }
    })?;

    for path in paths {
        watcher.watch(path, RecursiveMode::Recursive)?;
    }

    tokio::spawn(async move {
        events_tx.closed().await;
        drop(watcher);
    });

    Ok(events_rx)
}

async fn debounced_next<T>(s: &mut (impl Stream<Item = T> + Unpin)) -> Option<T> {
    let mut v = s.next().await;

    loop {
        let result = tokio::time::timeout(Duration::from_millis(50), s.next()).await;
        match result {
            Ok(Some(new)) => {
                v = Some(new);
            }
            Ok(None) => return v,
            Err(_) => return v,
        }
    }
}

pub async fn debounced_watch<T: Clone>(
    rx: &mut tokio::sync::watch::Receiver<T>,
    duration: Duration,
) -> Result<(), tokio::sync::watch::error::RecvError> {
    rx.changed().await?;

    let timer = sleep(duration);
    tokio::pin!(timer);

    loop {
        tokio::select! {
            // Wait for a change
            res = rx.changed() => {
                res?;
                timer.as_mut().reset(Instant::now() + duration);
            }

            // Timer completes
            _ = &mut timer => {
                return Ok(())
            }
        }
    }
}

/// Watch a single manifest dir.
async fn watch_single(cli: Arc<Cli>, config: Config) -> anyhow::Result<()> {
    let mut config = Arc::new(config);
    // Wait for run_events to run the stylance process.
    let (run_events_tx, mut run_events) = tokio::sync::watch::channel(config.clone());
    tokio::spawn({
        async move {
            while debounced_watch(&mut run_events, Duration::from_millis(50))
                .await
                .is_ok()
            {
                let config = run_events.borrow_and_update().clone();
                if let Ok(Err(e)) = spawn_blocking(move || run(&config)).await {
                    eprintln!("{e}");
                }
            }
        }
    });

    loop {
        // Watch Cargo.toml to update the current config.
        let mut watched_files = vec![config.manifest_dir.join("Cargo.toml")];

        // Also watch workspace Cargo.toml if the config inherits from it.
        if let Some(workspace_dir) = &config.workspace_dir {
            watched_files.push(workspace_dir.join("Cargo.toml"))
        }

        let mut cargo_toml_events = watch_files(&watched_files)?;

        // Watch the folders from the current config
        let mut folder_events = watch_folders(&config.folders)?;

        // With the events from the watched folder trigger run_events if they match the extensions of the config.
        let watch_folders = {
            let run_events_tx = run_events_tx.clone();
            let config = config.clone();
            async move {
                while let Some(path) = folder_events.recv().await {
                    let str_path = path.to_string_lossy();
                    if config.extensions.iter().any(|ext| str_path.ends_with(ext)) {
                        let _ = run_events_tx.send(config);
                        break;
                    }
                }
            }
        };

        // Run until the config has changed
        tokio::select! {
            _ = watch_folders => {},
            _ = cargo_toml_events.recv() => {},
        }

        // The cargo_toml_watcher triggered so wait a bit and reload the config.
        tokio::time::sleep(Duration::from_millis(50)).await;

        match load_config(&cli, &config.manifest_dir).await {
            Ok(new_config) => {
                config = Arc::new(new_config);
            }
            Err(e) => {
                eprintln!("{e}")
            }
        }

        // trigger a rebuild
        run_events_tx.send(config.clone())?;
    }
}
