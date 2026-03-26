use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use stylance_cli::{run_silent, Config};
use stylance_core::load_config;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

#[test]
fn test_single_manifest_dir() {
    let crate1 = fixtures_dir().join("crate1");
    let config = load_config(&crate1).unwrap();

    let mut visited = Vec::new();
    run_silent(&crate1, &config, |path| {
        visited.push(path.to_owned());
    })
    .unwrap();

    assert_eq!(visited.len(), 1);
    assert!(visited[0].to_string_lossy().contains("button.module.css"));
}

#[test]
fn test_multiple_manifest_dirs() {
    let crate1 = fixtures_dir().join("crate1");
    let crate2 = fixtures_dir().join("crate2");

    let configs: Vec<(PathBuf, Config)> = vec![
        (crate1.clone(), load_config(&crate1).unwrap()),
        (crate2.clone(), load_config(&crate2).unwrap()),
    ];

    let mut all_visited = Vec::new();
    for (manifest_dir, config) in &configs {
        run_silent(manifest_dir, config, |path| {
            all_visited.push(path.to_owned());
        })
        .unwrap();
    }

    assert_eq!(all_visited.len(), 2);

    let names: Vec<String> = all_visited
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(names.contains(&"button.module.css".to_string()));
    assert!(names.contains(&"card.module.css".to_string()));
}

#[test]
fn test_multiple_dirs_produce_distinct_output() {
    let crate1 = fixtures_dir().join("crate1");
    let crate2 = fixtures_dir().join("crate2");

    let tmpdir = std::env::temp_dir().join("stylance_test_multi");
    let _ = std::fs::remove_dir_all(&tmpdir);

    let output1 = tmpdir.join("crate1.css");
    let output2 = tmpdir.join("crate2.css");

    let mut config1 = load_config(&crate1).unwrap();
    config1.output_file = Some(output1.clone());

    let mut config2 = load_config(&crate2).unwrap();
    config2.output_file = Some(output2.clone());

    run_silent(&crate1, &config1, |_| {}).unwrap();
    run_silent(&crate2, &config2, |_| {}).unwrap();

    let content1 = std::fs::read_to_string(&output1).unwrap();
    let content2 = std::fs::read_to_string(&output2).unwrap();

    // Each output should contain transformed class names (with hashes)
    assert!(content1.contains("container-"));
    assert!(content1.contains("title-"));
    assert!(!content1.contains("wrapper-"));

    assert!(content2.contains("wrapper-"));
    assert!(content2.contains("header-"));
    assert!(!content2.contains("container-"));

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_cli_accepts_multiple_args() {
    // Verify the binary accepts multiple positional arguments
    let binary = env!("CARGO_BIN_EXE_stylance");
    let crate1 = fixtures_dir().join("crate1");
    let crate2 = fixtures_dir().join("crate2");

    let output = std::process::Command::new(binary)
        .arg(&crate1)
        .arg(&crate2)
        .output()
        .expect("failed to execute stylance binary");

    assert!(
        output.status.success(),
        "stylance with multiple dirs failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("button.module.css"));
    assert!(stdout.contains("card.module.css"));
}

#[test]
fn test_cli_output_file_override() {
    let binary = env!("CARGO_BIN_EXE_stylance");
    let crate1 = fixtures_dir().join("crate1");

    let tmpdir = std::env::temp_dir().join("stylance_test_output_override");
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(&tmpdir).unwrap();

    let output_file = tmpdir.join("override.css");

    let output = std::process::Command::new(binary)
        .arg(&crate1)
        .arg("--output-file")
        .arg(&output_file)
        .output()
        .expect("failed to execute stylance binary");

    assert!(
        output.status.success(),
        "stylance failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let contents = std::fs::read_to_string(&output_file).unwrap();
    assert!(
        contents.contains("container-"),
        "CLI --output-file override should produce CSS output"
    );

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_cli_folder_override() {
    let binary = env!("CARGO_BIN_EXE_stylance");
    let crate1 = fixtures_dir().join("crate1");

    let tmpdir = std::env::temp_dir().join("stylance_test_folder_override");
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(&tmpdir).unwrap();

    let output_file = tmpdir.join("folder_override.css");

    // Create an empty folder inside the crate to override with
    let empty_folder = crate1.join("empty_for_test");
    std::fs::create_dir_all(&empty_folder).unwrap();

    // Override folders to the empty dir — output should have no CSS classes
    let output = std::process::Command::new(binary)
        .arg(&crate1)
        .arg("--output-file")
        .arg(&output_file)
        .arg("--folder")
        .arg("empty_for_test")
        .output()
        .expect("failed to execute stylance binary");

    let _ = std::fs::remove_dir_all(&empty_folder);

    assert!(
        output.status.success(),
        "stylance failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let contents = std::fs::read_to_string(&output_file).unwrap();
    assert!(
        !contents.contains("container-"),
        "overriding --folder to an empty dir should not find CSS files"
    );

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_watch_produces_output_before_watching() {
    let binary = env!("CARGO_BIN_EXE_stylance");
    let crate1 = fixtures_dir().join("crate1");

    let tmpdir = std::env::temp_dir().join("stylance_test_watch_initial");
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(&tmpdir).unwrap();

    let output_file = tmpdir.join("watch_initial.css");

    let mut child = std::process::Command::new(binary)
        .arg(&crate1)
        .arg("--output-file")
        .arg(&output_file)
        .arg("--watch")
        .spawn()
        .expect("failed to start stylance");

    // Wait for the initial run to produce output
    let start = Instant::now();
    let mut found = false;
    while start.elapsed() < Duration::from_secs(5) {
        if output_file.exists() {
            let contents = std::fs::read_to_string(&output_file).unwrap();
            if contents.contains("container-") {
                found = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    child.kill().unwrap();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&tmpdir);

    assert!(
        found,
        "watch mode should produce output before entering the watch loop"
    );
}

#[test]
fn test_watch_exits_when_a_watcher_fails() {
    let binary = env!("CARGO_BIN_EXE_stylance");
    let crate1 = fixtures_dir().join("crate1");

    // Create a crate whose watched folders don't exist on disk.
    // The initial run will succeed (WalkDir silently skips missing dirs)
    // but the file watcher will fail to watch a nonexistent path,
    // which should cause the entire process to exit.
    let tmpdir = std::env::temp_dir().join("stylance_test_watch_exit");
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(&tmpdir).unwrap();
    std::fs::write(
        tmpdir.join("Cargo.toml"),
        r#"
[package]
name = "bad-crate"
version = "0.1.0"

[package.metadata.stylance]
output_file = "output.css"
folders = ["nonexistent_folder"]
"#,
    )
    .unwrap();

    let mut child = std::process::Command::new(binary)
        .arg(&crate1)
        .arg(&tmpdir)
        .arg("--watch")
        .spawn()
        .expect("failed to start stylance");

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                assert!(!status.success(), "process should exit with an error");
                break;
            }
            Ok(None) => {
                if start.elapsed() > Duration::from_secs(10) {
                    child.kill().unwrap();
                    panic!("stylance --watch did not exit when a watcher failed");
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => panic!("Error waiting for process: {e}"),
        }
    }

    let _ = std::fs::remove_dir_all(&tmpdir);
}
