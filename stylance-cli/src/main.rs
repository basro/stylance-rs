use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use stylance_cli::{load_and_modify_crate, write_output};
use stylance_core::{Config, ModifyCssResult};

use clap::Parser;
use notify::{Event, RecursiveMode, Watcher};
use tokio::{
    sync::mpsc,
    task::JoinSet,
    time::{sleep, Instant},
};

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

fn print_files(files: &[ModifyCssResult]) {
    for file in files {
        println!("{}", file.path.to_string_lossy());
    }
}

// We are using tokio mainly for the ease of implementing debouncing and cancellation.
// It is alright to call io blocking functions in async functions of this app.

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut crate_states = Vec::new();
    for manifest_dir in &cli.manifest_dirs {
        let config = Arc::new(load_config(&cli, manifest_dir)?);
        let files = load_and_modify_crate(&config)?;
        print_files(&files);
        crate_states.push(CrateState { config, files });
    }

    write_output(
        &crate_states
            .iter()
            .map(|c| (&*c.config, c.files.as_slice()))
            .collect::<Vec<_>>(),
    )?;

    if cli.watch {
        let cli = Arc::new(cli);

        // Spawn one independent watch task per manifest dir, as each crate
        // has its own config, folders, and output.
        let mut set = JoinSet::new();

        let (tx, rx) = mpsc::unbounded_channel();

        for (crate_idx, CrateState { config, .. }) in crate_states.iter().enumerate() {
            let cli = cli.clone();
            set.spawn(watch_single(cli, config.clone(), crate_idx, tx.clone()));
        }

        set.spawn(watch_crates(crate_states, rx));

        // If any watcher ends (only happens on error), abort the rest and exit.
        if let Some(result) = set.join_next().await {
            set.abort_all();
            result??;
        }
    }

    Ok(())
}

fn load_config(cli: &Cli, manifest_dir: &Path) -> anyhow::Result<Config> {
    let mut config = Config::load(manifest_dir.to_owned())?;

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

struct CrateState {
    config: Arc<Config>,
    files: Vec<ModifyCssResult>,
}

const DEBOUNCE_DURATION: Duration = Duration::from_millis(50);

async fn watch_crates(
    mut states: Vec<CrateState>,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<(usize, Arc<Config>)>,
) -> anyhow::Result<()> {
    while let Some((idx, config)) = rx.recv().await {
        // Debounce logic:
        // Accumulate reeived until enought time elapses since the last recv.
        // If the update is for the same crate (idx) we discard the old value.

        let mut received = HashMap::from([(idx, config)]);
        let timer = sleep(DEBOUNCE_DURATION);
        tokio::pin!(timer);

        loop {
            tokio::select! {
                // Wait for a change
                res = rx.recv() => {
                    match res {
                        Some((idx, config)) => {
                            received.insert(idx, config);
                        }
                        None => {
                            return Ok(());
                        }
                    }
                    timer.as_mut().reset(Instant::now() + DEBOUNCE_DURATION);
                }

                // Timer completes
                _ = &mut timer => {
                    break;
                }
            }
        }

        // Debounce ended, received contains 1 or more crates.

        for (idx, config) in received {
            match load_and_modify_crate(&config) {
                Ok(modified) => {
                    print_files(&modified);
                    states[idx] = CrateState {
                        config: config.clone(),
                        files: modified,
                    };
                }
                Err(e) => {
                    eprintln!("{e}");
                }
            };
        }

        write_output(
            &states
                .iter()
                .map(|c| (&*c.config, c.files.as_slice()))
                .collect::<Vec<_>>(),
        )?;
    }

    Ok(())
}

/// Watch a single manifest dir.
async fn watch_single(
    cli: Arc<Cli>,
    mut config: Arc<Config>,
    crate_idx: usize,
    build_tx: mpsc::UnboundedSender<(usize, Arc<Config>)>,
) -> anyhow::Result<()> {
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
        let watch_folders_fut = {
            let build_tx = build_tx.clone();
            let config = config.clone();
            async move {
                while let Some(path) = folder_events.recv().await {
                    let str_path = path.to_string_lossy();
                    if config.extensions.iter().any(|ext| str_path.ends_with(ext)) {
                        let _ = build_tx.send((crate_idx, config));
                        break;
                    }
                }
            }
        };

        // Run until the config has changed
        tokio::select! {
            _ = watch_folders_fut => {},
            _ = cargo_toml_events.recv() => {},
        }

        // The cargo_toml_watcher triggered so wait a bit and reload the config.
        tokio::time::sleep(Duration::from_millis(50)).await;

        match load_config(&cli, &config.manifest_dir) {
            Ok(new_config) => {
                config = Arc::new(new_config);
            }
            Err(e) => {
                eprintln!("{e}")
            }
        }

        // trigger a rebuild
        build_tx.send((crate_idx, config.clone()))?;
    }
}
