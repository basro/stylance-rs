use anyhow::bail;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use stylance_cli::{
    build_crate, build_output_file_content, write_output_dir, write_output_file, CrateBuilder,
    ModifyCssResult,
};
use stylance_core::{load_config, Config};

use clap::Parser;
use notify::{Event, RecursiveMode, Watcher};
use tokio::{
    sync::mpsc,
    task::{spawn_blocking, JoinSet},
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

struct RunParams {
    manifest_dir: PathBuf,
    config: Config,
}

/// Check that no two crates share the same output_dir (not supported).
/// Shared output_file is allowed and handled by concatenation.
fn check_output_dir_collisions(all_params: &[RunParams]) -> anyhow::Result<()> {
    let mut seen_dirs: HashMap<PathBuf, &Path> = HashMap::new();

    for params in all_params {
        if let Some(output_dir) = &params.config.output_dir {
            if let Some(prev) = seen_dirs.insert(output_dir.clone(), &params.manifest_dir) {
                bail!(
                    "Multiple crates share the same output_dir: {}\n  - {}\n  - {}",
                    output_dir.display(),
                    prev.display(),
                    params.manifest_dir.display(),
                );
            }
        }
    }

    Ok(())
}

/// Write all outputs, concatenating results for crates that share an output_file.
/// Crate ordering follows CLI argument order.
fn write_all_outputs(
    all_params: &[RunParams],
    all_results: &[Vec<ModifyCssResult>],
) -> anyhow::Result<()> {
    // Group crate indices by output_file path
    let mut output_file_groups: HashMap<&Path, Vec<usize>> = HashMap::new();
    for (i, params) in all_params.iter().enumerate() {
        if let Some(output_file) = &params.config.output_file {
            output_file_groups
                .entry(output_file.as_path())
                .or_default()
                .push(i);
        }
    }

    // Write each output_file group once
    let mut written_files: HashSet<&Path> = HashSet::new();
    for (output_file, group) in &output_file_groups {
        if written_files.insert(output_file) {
            let crate_results: Vec<&[ModifyCssResult]> =
                group.iter().map(|&i| all_results[i].as_slice()).collect();
            let scss_prelude = all_params[group[0]].config.scss_prelude.as_deref();
            write_output_file(output_file, scss_prelude, &crate_results)?;
        }
    }

    // Write output_dirs (per-crate, no sharing)
    for (i, params) in all_params.iter().enumerate() {
        if let Some(output_dir) = &params.config.output_dir {
            write_output_dir(
                output_dir,
                params.config.scss_prelude.as_deref(),
                &all_results[i],
            )?;
        }
    }

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut all_params = Vec::new();
    for manifest_dir in &cli.manifest_dirs {
        let params = make_run_params(&cli, manifest_dir).await?;
        all_params.push(params);
    }

    check_output_dir_collisions(&all_params)?;

    // Build all crates
    let mut all_results: Vec<Vec<ModifyCssResult>> = Vec::new();
    for params in &all_params {
        println!("Running stylance");
        let results = build_crate(&params.manifest_dir, &params.config, |path| {
            println!("{}", path.display());
        })?;
        all_results.push(results);
    }

    // Write outputs with shared output_file support
    write_all_outputs(&all_params, &all_results)?;

    if cli.watch {
        let cli = Arc::new(cli);
        let (agg_tx, agg_rx) = mpsc::unbounded_channel();

        // Initialize CrateBuilders from existing results (avoids re-reading files)
        let builders: Vec<CrateBuilder> = all_params
            .into_iter()
            .zip(all_results)
            .map(|(params, results)| {
                CrateBuilder::from_results(params.manifest_dir, params.config, results)
            })
            .collect();

        // Spawn aggregator
        let agg_handle = tokio::spawn(run_aggregator(agg_rx));

        // Spawn one watcher per crate
        let mut set = JoinSet::new();
        for (i, builder) in builders.into_iter().enumerate() {
            let cli = cli.clone();
            let tx = agg_tx.clone();
            set.spawn(watch_crate(cli, i, builder, tx));
        }
        drop(agg_tx); // aggregator exits when all watchers drop their senders

        // If any watcher ends (only happens on error), abort the rest and exit.
        tokio::select! {
            result = async { set.join_next().await } => {
                if let Some(result) = result {
                    set.abort_all();
                    result??;
                }
            }
            result = agg_handle => {
                result??;
            }
        }
    }

    Ok(())
}

async fn make_run_params(cli: &Cli, manifest_dir: &Path) -> anyhow::Result<RunParams> {
    let manifest_dir_buf = manifest_dir.to_path_buf();
    let mut config = spawn_blocking(move || load_config(&manifest_dir_buf)).await??;

    config.output_file = cli
        .output_file
        .clone()
        .or_else(|| config.output_file.as_ref().map(|p| manifest_dir.join(p)));

    config.output_dir = cli
        .output_dir
        .clone()
        .or_else(|| config.output_dir.as_ref().map(|p| manifest_dir.join(p)));

    if !cli.folder.is_empty() {
        config.folders = Some(cli.folder.clone());
    }

    Ok(RunParams {
        manifest_dir: manifest_dir.to_path_buf(),
        config,
    })
}

// ---------------------------------------------------------------------------
// Aggregator: receives build results and writes combined outputs
// ---------------------------------------------------------------------------

struct CrateBuildResult {
    crate_index: usize,
    config: Config,
    results: Vec<ModifyCssResult>,
}

async fn run_aggregator(mut rx: mpsc::UnboundedReceiver<CrateBuildResult>) -> anyhow::Result<()> {
    let mut configs: HashMap<usize, Config> = HashMap::new();
    let mut results: HashMap<usize, Vec<ModifyCssResult>> = HashMap::new();
    let mut last_written: HashMap<PathBuf, String> = HashMap::new();

    while let Some(msg) = rx.recv().await {
        configs.insert(msg.crate_index, msg.config);
        results.insert(msg.crate_index, msg.results);
        println!("Running stylance");

        // Group crate indices by output_file
        let mut output_file_groups: HashMap<&Path, Vec<usize>> = HashMap::new();
        for (&i, config) in &configs {
            if let Some(output_file) = &config.output_file {
                output_file_groups
                    .entry(output_file.as_path())
                    .or_default()
                    .push(i);
            }
        }

        // Ensure stable ordering within each group
        for group in output_file_groups.values_mut() {
            group.sort();
        }

        // Write each output_file group (content-aware: skip if unchanged)
        for (output_file, group) in &output_file_groups {
            let crate_results: Vec<&[ModifyCssResult]> = group
                .iter()
                .map(|i| results.get(i).map(|r| r.as_slice()).unwrap_or(&[]))
                .collect();
            let scss_prelude = configs[&group[0]].scss_prelude.as_deref();

            let new_content = build_output_file_content(output_file, scss_prelude, &crate_results);

            if last_written.get(*output_file) != Some(&new_content) {
                if let Err(e) = write_output_file(output_file, scss_prelude, &crate_results) {
                    eprintln!("{e}");
                } else {
                    last_written.insert(output_file.to_path_buf(), new_content);
                }
            }
        }

        // Write output_dirs (per-crate, no sharing)
        if let Some(config) = configs.get(&msg.crate_index) {
            if let Some(output_dir) = &config.output_dir {
                let crate_results = results
                    .get(&msg.crate_index)
                    .map(|r| r.as_slice())
                    .unwrap_or(&[]);
                if let Err(e) =
                    write_output_dir(output_dir, config.scss_prelude.as_deref(), crate_results)
                {
                    eprintln!("{e}");
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// File watching helpers
// ---------------------------------------------------------------------------

fn watch_file(path: &Path) -> anyhow::Result<mpsc::UnboundedReceiver<()>> {
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

    watcher.watch(path, RecursiveMode::NonRecursive)?;

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

/// Collect changed file paths during a debounce window, filtering by extension.
async fn debounced_collect(
    rx: &mut mpsc::UnboundedReceiver<PathBuf>,
    extensions: &[String],
) -> Option<HashSet<PathBuf>> {
    let mut paths = HashSet::new();

    // Wait for first matching event
    loop {
        let path = rx.recv().await?;
        let str_path = path.to_string_lossy();
        if extensions.iter().any(|ext| str_path.ends_with(ext)) {
            paths.insert(path);
            break;
        }
    }

    // Collect during debounce window
    loop {
        match tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
            Ok(Some(path)) => {
                let str_path = path.to_string_lossy();
                if extensions.iter().any(|ext| str_path.ends_with(ext)) {
                    paths.insert(path);
                }
            }
            Ok(None) => return None,
            Err(_) => return Some(paths),
        }
    }
}

// ---------------------------------------------------------------------------
// Per-crate watch loop with incremental rebuilds
// ---------------------------------------------------------------------------

async fn watch_crate(
    cli: Arc<Cli>,
    crate_index: usize,
    mut builder: CrateBuilder,
    output_tx: mpsc::UnboundedSender<CrateBuildResult>,
) -> anyhow::Result<()> {
    let manifest_dir = builder.manifest_dir().to_path_buf();

    // Watch Cargo.toml for config changes
    let cargo_toml_events = watch_file(&manifest_dir.join("Cargo.toml").canonicalize()?)?;
    let (config_tx, mut config_rx) = mpsc::channel::<Config>(1);
    let manifest_dir_clone = manifest_dir.clone();
    tokio::spawn(async move {
        let mut stream = tokio_stream::wrappers::UnboundedReceiverStream::new(cargo_toml_events);
        while debounced_next(&mut stream).await.is_some() {
            match make_run_params(&cli, &manifest_dir_clone).await {
                Ok(new_params) => {
                    if config_tx.send(new_params.config).await.is_err() {
                        return;
                    }
                }
                Err(e) => eprintln!("{e}"),
            }
        }
    });

    loop {
        // Watch folders with current config
        let folder_paths: Vec<PathBuf> = builder
            .config()
            .folders()
            .iter()
            .map(|f| manifest_dir.join(f))
            .collect();
        let mut folder_events = watch_folders(&folder_paths)?;

        let extensions: Vec<String> = builder.config().extensions().to_vec();

        // Wait for either file changes or config changes
        enum WatchEvent {
            FilesChanged(HashSet<PathBuf>),
            ConfigChanged(Config),
        }

        let event = tokio::select! {
            changed = debounced_collect(&mut folder_events, &extensions) => {
                match changed {
                    Some(paths) => WatchEvent::FilesChanged(paths),
                    None => return Ok(()),
                }
            }
            config = config_rx.recv() => {
                match config {
                    Some(config) => WatchEvent::ConfigChanged(config),
                    None => return Ok(()),
                }
            }
        };

        // Run the rebuild in a blocking task (returns builder even on error)
        let (b, result) = match event {
            WatchEvent::FilesChanged(changed) => {
                spawn_blocking(move || {
                    let rebuild = builder
                        .rebuild_changed(&changed)
                        .and_then(|()| builder.output());
                    (builder, rebuild)
                })
                .await?
            }
            WatchEvent::ConfigChanged(config) => {
                spawn_blocking(move || {
                    let rebuild = builder
                        .update_config(config)
                        .and_then(|()| builder.output());
                    (builder, rebuild)
                })
                .await?
            }
        };
        builder = b;

        match result {
            Ok(results) => {
                let _ = output_tx.send(CrateBuildResult {
                    crate_index,
                    config: builder.config().clone(),
                    results,
                });
            }
            Err(e) => eprintln!("{e}"),
        }
    }
}
