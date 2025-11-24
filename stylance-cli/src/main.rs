use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use stylance_cli::run;
use stylance_core::{load_config, Config};

use clap::Parser;
use notify::{Event, RecursiveMode, Watcher};
use tokio::{sync::mpsc, task::spawn_blocking};
use tokio_stream::{Stream, StreamExt};

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

struct RunParams {
    manifest_dir: PathBuf,
    config: Config,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let run_params = make_run_params(&cli).await?;

    run(&run_params.manifest_dir, &run_params.config)?;

    if cli.watch {
        watch(cli, run_params).await?;
    }

    Ok(())
}

async fn make_run_params(cli: &Cli) -> anyhow::Result<RunParams> {
    let manifest_dir = cli.manifest_dir.clone();
    let mut config = spawn_blocking(move || load_config(&manifest_dir)).await??;

    config.output_file = cli.output_file.clone().or_else(|| {
        config
            .output_file
            .as_ref()
            .map(|p| cli.manifest_dir.join(p))
    });

    config.output_dir = cli
        .output_dir
        .clone()
        .or_else(|| config.output_dir.as_ref().map(|p| cli.manifest_dir.join(p)));

    if !cli.folder.is_empty() {
        config.folders.clone_from(&cli.folder);
    }

    Ok(RunParams {
        manifest_dir: cli.manifest_dir.clone(),
        config,
    })
}

fn watch_file(path: &Path) -> anyhow::Result<mpsc::UnboundedReceiver<()>> {
    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher({
        let events_tx = events_tx.clone();
        move |_| {
            let _ = events_tx.send(());
        }
    })?;

    watcher.watch(path, RecursiveMode::NonRecursive)?;

    tokio::spawn(async move {
        events_tx.closed().await;
        drop(watcher);
    });

    Ok(events_rx)
}

fn watch_folders(paths: &Vec<PathBuf>) -> anyhow::Result<mpsc::UnboundedReceiver<PathBuf>> {
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

async fn debounced_next(s: &mut (impl Stream<Item = ()> + Unpin)) -> Option<()> {
    s.next().await;

    loop {
        let result = tokio::time::timeout(Duration::from_millis(50), s.next()).await;
        match result {
            Ok(Some(_)) => {}
            Ok(None) => return None,
            Err(_) => return Some(()),
        }
    }
}

async fn watch(cli: Cli, run_params: RunParams) -> anyhow::Result<()> {
    let (run_params_tx, mut run_params) = tokio::sync::watch::channel(Arc::new(run_params));

    let manifest_dir = cli.manifest_dir.clone();

    // Watch Cargo.toml to update the current run_params.
    let cargo_toml_events = watch_file(&manifest_dir.join("Cargo.toml").canonicalize()?)?;
    tokio::spawn(async move {
        let mut stream = tokio_stream::wrappers::UnboundedReceiverStream::new(cargo_toml_events);
        while debounced_next(&mut stream).await.is_some() {
            match make_run_params(&cli).await {
                Ok(new_params) => {
                    if run_params_tx.send(Arc::new(new_params)).is_err() {
                        return;
                    };
                }
                Err(e) => {
                    eprintln!("{e}");
                }
            }
        }
    });

    // Wait for run_events to run the stylance process.
    let (run_events_tx, run_events) = mpsc::channel(1);
    tokio::spawn({
        let run_params = run_params.clone();
        async move {
            let mut stream = tokio_stream::wrappers::ReceiverStream::new(run_events);
            while (debounced_next(&mut stream).await).is_some() {
                let run_params = run_params.borrow().clone();
                if let Ok(Err(e)) =
                    spawn_blocking(move || run(&run_params.manifest_dir, &run_params.config)).await
                {
                    eprintln!("{e}");
                }
            }
        }
    });

    loop {
        // Watch the folders from the current run_params
        let mut events = watch_folders(
            &run_params
                .borrow()
                .config
                .folders
                .iter()
                .map(|f| manifest_dir.join(f))
                .collect(),
        )?;

        // With the events from the watched folder trigger run_events if they match the extensions of the config.
        let watch_folders = {
            let run_params = run_params.borrow().clone();
            let run_events_tx = run_events_tx.clone();
            async move {
                while let Some(path) = events.recv().await {
                    let str_path = path.to_string_lossy();
                    if run_params
                        .config
                        .extensions
                        .iter()
                        .any(|ext| str_path.ends_with(ext))
                    {
                        let _ = run_events_tx.try_send(());
                        break;
                    }
                }
            }
        };

        // Run until the config has changed
        tokio::select! {
            _ = watch_folders => {},
            _ = run_params.changed() => {
                let _ = run_events_tx.try_send(()); // Config changed so lets trigger a run
            },
        }
    }
}
