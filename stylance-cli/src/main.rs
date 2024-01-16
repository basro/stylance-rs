use std::{
    borrow::Cow,
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use stylance_core::load_config;

use clap::Parser;
use notify::{Event, RecursiveMode, Watcher};
use tokio::{sync::mpsc, task::spawn_blocking};
use tokio_stream::{Stream, StreamExt};
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let run_config = make_run_config(&cli).await?;

    run(&run_config)?;

    if cli.watch {
        watch(cli, run_config).await?;
    }

    Ok(())
}

struct RunConfig {
    manifest_dir: PathBuf,
    output_file: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    extensions: Vec<String>,
    folders: Vec<PathBuf>,
    scss_prelude: Option<String>,
}

async fn make_run_config(cli: &Cli) -> anyhow::Result<RunConfig> {
    let manifest_dir = cli.manifest_dir.clone();
    let config = spawn_blocking(move || load_config(&manifest_dir)).await??;

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
        scss_prelude: config.scss_prelude,
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
            if let Ok(e) = e {
                for path in e.paths {
                    if events_tx.send(path).is_err() {
                        break;
                    }
                }
            }
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

async fn watch(cli: Cli, run_config: RunConfig) -> anyhow::Result<()> {
    let (run_config_tx, mut run_config) = tokio::sync::watch::channel(Arc::new(run_config));

    // Watch Cargo.toml to update the current run_config.
    let cargo_toml_events = watch_file(&cli.manifest_dir.join("Cargo.toml").canonicalize()?)?;
    tokio::spawn(async move {
        let mut stream = tokio_stream::wrappers::UnboundedReceiverStream::new(cargo_toml_events);
        while debounced_next(&mut stream).await.is_some() {
            match make_run_config(&cli).await {
                Ok(new_config) => {
                    if run_config_tx.send(Arc::new(new_config)).is_err() {
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
        let run_config = run_config.clone();
        async move {
            let mut stream = tokio_stream::wrappers::ReceiverStream::new(run_events);
            while (debounced_next(&mut stream).await).is_some() {
                let run_config = run_config.borrow().clone();
                if let Ok(Err(e)) = spawn_blocking(move || run(&run_config)).await {
                    eprintln!("{e}");
                }
            }
        }
    });

    loop {
        // Watch the folders from the current run_config
        let mut events = watch_folders(&run_config.borrow().folders)?;

        // With the events from the watched folder trigger run_events if they match the extensions of the config.
        let watch_folders = {
            let run_config = run_config.borrow().clone();
            let run_events_tx = run_events_tx.clone();
            async move {
                while let Some(path) = events.recv().await {
                    let str_path = path.to_string_lossy();
                    if run_config
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
            _ = run_config.changed() => {
                let _ = run_events_tx.try_send(()); // Config changed so lets trigger a run
            },
        }
    }
}
