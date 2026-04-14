use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use stylance_cli::{
    build_crate, build_output_file_content, run_silent, write_output_file, Config, CrateBuilder,
};
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
fn test_workspace_config_inheritance() {
    // Create a workspace with two crates that opt into workspace config
    let tmpdir = std::env::temp_dir().join("stylance_test_workspace");
    let _ = std::fs::remove_dir_all(&tmpdir);

    let crate_a = tmpdir.join("crates").join("crate_a");
    let crate_b = tmpdir.join("crates").join("crate_b");
    std::fs::create_dir_all(crate_a.join("src")).unwrap();
    std::fs::create_dir_all(crate_b.join("src")).unwrap();

    // Workspace Cargo.toml with shared stylance config
    std::fs::write(
        tmpdir.join("Cargo.toml"),
        r#"
[workspace]
members = ["crates/*"]

[workspace.metadata.stylance]
extensions = [".module.css"]
hash_len = 5
"#,
    )
    .unwrap();

    // Crate A: opts into workspace config
    std::fs::write(
        crate_a.join("Cargo.toml"),
        r#"
[package]
name = "crate-a"
version = "0.1.0"

[package.metadata.stylance]
workspace = true
"#,
    )
    .unwrap();

    // Crate B: opts into workspace config
    std::fs::write(
        crate_b.join("Cargo.toml"),
        r#"
[package]
name = "crate-b"
version = "0.1.0"

[package.metadata.stylance]
workspace = true
"#,
    )
    .unwrap();

    std::fs::write(crate_a.join("src/style.module.css"), ".btn { color: red; }").unwrap();
    std::fs::write(
        crate_b.join("src/style.module.css"),
        ".btn { color: blue; }",
    )
    .unwrap();

    let config_a = load_config(&crate_a).unwrap();
    let config_b = load_config(&crate_b).unwrap();

    // Workspace config should be inherited: hash_len = 5
    assert_eq!(config_a.hash_len(), 5);
    assert_eq!(config_b.hash_len(), 5);

    // extensions should come from workspace
    assert_eq!(config_a.extensions(), &[".module.css"]);

    // hash_root_path should be implicitly set to workspace root
    assert!(config_a.hash_root_path.is_some());
    assert!(config_b.hash_root_path.is_some());

    // Same relative CSS path in both crates should produce different hashes
    let output_a = tmpdir.join("a.css");
    let output_b = tmpdir.join("b.css");

    let mut config_a = config_a;
    config_a.output_file = Some(output_a.clone());
    let mut config_b = config_b;
    config_b.output_file = Some(output_b.clone());

    run_silent(&crate_a, &config_a, |_| {}).unwrap();
    run_silent(&crate_b, &config_b, |_| {}).unwrap();

    let content_a = std::fs::read_to_string(&output_a).unwrap();
    let content_b = std::fs::read_to_string(&output_b).unwrap();

    // Both should have transformed classes
    assert!(content_a.contains(".btn-"));
    assert!(content_b.contains(".btn-"));

    // But hashes should differ (different crates, same relative path)
    assert_ne!(content_a, content_b);

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_workspace_config_crate_override() {
    // Crate-level config should override workspace config
    let tmpdir = std::env::temp_dir().join("stylance_test_ws_override");
    let _ = std::fs::remove_dir_all(&tmpdir);

    let crate_dir = tmpdir.join("my_crate");
    std::fs::create_dir_all(crate_dir.join("src")).unwrap();

    std::fs::write(
        tmpdir.join("Cargo.toml"),
        r#"
[workspace]
members = ["my_crate"]

[workspace.metadata.stylance]
hash_len = 5
"#,
    )
    .unwrap();

    // Crate overrides hash_len
    std::fs::write(
        crate_dir.join("Cargo.toml"),
        r#"
[package]
name = "my-crate"
version = "0.1.0"

[package.metadata.stylance]
workspace = true
hash_len = 10
"#,
    )
    .unwrap();

    let config = load_config(&crate_dir).unwrap();
    assert_eq!(config.hash_len(), 10);

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_workspace_false_no_inheritance() {
    // Without workspace = true, no inheritance should happen
    let tmpdir = std::env::temp_dir().join("stylance_test_ws_false");
    let _ = std::fs::remove_dir_all(&tmpdir);

    let crate_dir = tmpdir.join("my_crate");
    std::fs::create_dir_all(crate_dir.join("src")).unwrap();

    std::fs::write(
        tmpdir.join("Cargo.toml"),
        r#"
[workspace]
members = ["my_crate"]

[workspace.metadata.stylance]
hash_len = 5
"#,
    )
    .unwrap();

    std::fs::write(
        crate_dir.join("Cargo.toml"),
        r#"
[package]
name = "my-crate"
version = "0.1.0"
"#,
    )
    .unwrap();

    let config = load_config(&crate_dir).unwrap();
    // Should get the default hash_len (7), not the workspace one (5)
    assert_eq!(config.hash_len(), 7);
    assert!(config.hash_root_path.is_none());

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_hash_root_path_changes_hash() {
    let crate1 = fixtures_dir().join("crate1");

    let tmpdir = std::env::temp_dir().join("stylance_test_hash_root");
    let _ = std::fs::remove_dir_all(&tmpdir);

    // Run without hash_root_path
    let output_default = tmpdir.join("default.css");
    let mut config = load_config(&crate1).unwrap();
    config.output_file = Some(output_default.clone());
    run_silent(&crate1, &config, |_| {}).unwrap();
    let content_default = std::fs::read_to_string(&output_default).unwrap();

    // Run with hash_root_path pointing to the fixtures dir (parent of crate1)
    let output_custom = tmpdir.join("custom.css");
    let mut config2 = load_config(&crate1).unwrap();
    config2.output_file = Some(output_custom.clone());
    config2.hash_root_path = Some(PathBuf::from("../"));
    run_silent(&crate1, &config2, |_| {}).unwrap();
    let content_custom = std::fs::read_to_string(&output_custom).unwrap();

    // Both should contain transformed class names
    assert!(content_default.contains("container-"));
    assert!(content_custom.contains("container-"));

    // But the hashes should differ because the relative path changed
    assert_ne!(
        content_default, content_custom,
        "hash_root_path should produce different hashes"
    );

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_hash_root_path_makes_same_filename_unique() {
    // Simulate two crates with the same relative CSS file path (src/style.module.css)
    // When both use hash_root_path pointing to a common root, they should get different hashes.
    let tmpdir = std::env::temp_dir().join("stylance_test_hash_root_unique");
    let _ = std::fs::remove_dir_all(&tmpdir);

    let crate_a = tmpdir.join("crate_a");
    let crate_b = tmpdir.join("crate_b");
    std::fs::create_dir_all(crate_a.join("src")).unwrap();
    std::fs::create_dir_all(crate_b.join("src")).unwrap();

    // Same CSS content and same relative path in both crates
    let css_content = ".btn { color: red; }";
    std::fs::write(crate_a.join("src/style.module.css"), css_content).unwrap();
    std::fs::write(crate_b.join("src/style.module.css"), css_content).unwrap();

    // Without hash_root_path: both crates produce the same hash
    let config_no_root = Config {
        output_file: Some(tmpdir.join("a_default.css")),
        ..Config::default()
    };
    run_silent(&crate_a, &config_no_root, |_| {}).unwrap();
    let out_a_default = std::fs::read_to_string(tmpdir.join("a_default.css")).unwrap();

    let config_no_root_b = Config {
        output_file: Some(tmpdir.join("b_default.css")),
        ..Config::default()
    };
    run_silent(&crate_b, &config_no_root_b, |_| {}).unwrap();
    let out_b_default = std::fs::read_to_string(tmpdir.join("b_default.css")).unwrap();

    assert_eq!(
        out_a_default, out_b_default,
        "without hash_root_path, same relative paths should produce same hashes"
    );

    // With hash_root_path pointing to the common parent: hashes should differ
    let config_a = Config {
        output_file: Some(tmpdir.join("a_rooted.css")),
        hash_root_path: Some(PathBuf::from("../")),
        ..Config::default()
    };
    run_silent(&crate_a, &config_a, |_| {}).unwrap();
    let out_a_rooted = std::fs::read_to_string(tmpdir.join("a_rooted.css")).unwrap();

    let config_b = Config {
        output_file: Some(tmpdir.join("b_rooted.css")),
        hash_root_path: Some(PathBuf::from("../")),
        ..Config::default()
    };
    run_silent(&crate_b, &config_b, |_| {}).unwrap();
    let out_b_rooted = std::fs::read_to_string(tmpdir.join("b_rooted.css")).unwrap();

    assert_ne!(
        out_a_rooted, out_b_rooted,
        "with hash_root_path to common parent, different crates should produce different hashes"
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

// ---------------------------------------------------------------------------
// Shared output_file tests
// ---------------------------------------------------------------------------

#[test]
fn test_shared_output_file_concatenates_css() {
    let crate1 = fixtures_dir().join("crate1");
    let crate2 = fixtures_dir().join("crate2");

    let tmpdir = std::env::temp_dir().join("stylance_test_shared_output");
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(&tmpdir).unwrap();

    let shared_output = tmpdir.join("bundle.css");

    let mut config1 = load_config(&crate1).unwrap();
    config1.output_file = Some(shared_output.clone());

    let mut config2 = load_config(&crate2).unwrap();
    config2.output_file = Some(shared_output.clone());

    let results1 = build_crate(&crate1, &config1, |_| {}).unwrap();
    let results2 = build_crate(&crate2, &config2, |_| {}).unwrap();

    write_output_file(
        &shared_output,
        None,
        &[results1.as_slice(), results2.as_slice()],
    )
    .unwrap();

    let content = std::fs::read_to_string(&shared_output).unwrap();

    // Both crates' CSS should be present
    assert!(
        content.contains("container-"),
        "should contain crate1 class"
    );
    assert!(content.contains("title-"), "should contain crate1 class");
    assert!(content.contains("wrapper-"), "should contain crate2 class");
    assert!(content.contains("header-"), "should contain crate2 class");

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_shared_output_file_preserves_crate_order() {
    let crate1 = fixtures_dir().join("crate1");
    let crate2 = fixtures_dir().join("crate2");

    let mut config1 = load_config(&crate1).unwrap();
    config1.output_file = Some(PathBuf::from("dummy.css"));

    let mut config2 = load_config(&crate2).unwrap();
    config2.output_file = Some(PathBuf::from("dummy.css"));

    let results1 = build_crate(&crate1, &config1, |_| {}).unwrap();
    let results2 = build_crate(&crate2, &config2, |_| {}).unwrap();

    // Order: crate1 first, then crate2
    let content_1_2 = build_output_file_content(
        Path::new("dummy.css"),
        None,
        &[results1.as_slice(), results2.as_slice()],
    );

    // Order: crate2 first, then crate1
    let content_2_1 = build_output_file_content(
        Path::new("dummy.css"),
        None,
        &[results2.as_slice(), results1.as_slice()],
    );

    // Both orderings should contain all classes
    assert!(content_1_2.contains("container-"));
    assert!(content_1_2.contains("wrapper-"));

    // But the order of CSS blocks should differ
    let pos_container_1_2 = content_1_2.find("container-").unwrap();
    let pos_wrapper_1_2 = content_1_2.find("wrapper-").unwrap();
    assert!(
        pos_container_1_2 < pos_wrapper_1_2,
        "crate1 CSS should come before crate2 CSS"
    );

    let pos_container_2_1 = content_2_1.find("container-").unwrap();
    let pos_wrapper_2_1 = content_2_1.find("wrapper-").unwrap();
    assert!(
        pos_wrapper_2_1 < pos_container_2_1,
        "reversed order: crate2 CSS should come before crate1 CSS"
    );
}

#[test]
fn test_shared_output_file_via_cli() {
    let binary = env!("CARGO_BIN_EXE_stylance");
    let crate1 = fixtures_dir().join("crate1");
    let crate2 = fixtures_dir().join("crate2");

    let tmpdir = std::env::temp_dir().join("stylance_test_shared_cli");
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(&tmpdir).unwrap();

    let shared_output = tmpdir.join("shared.css");

    let output = std::process::Command::new(binary)
        .arg(&crate1)
        .arg(&crate2)
        .arg("--output-file")
        .arg(&shared_output)
        .output()
        .expect("failed to execute stylance binary");

    assert!(
        output.status.success(),
        "stylance with shared --output-file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = std::fs::read_to_string(&shared_output).unwrap();
    assert!(
        content.contains("container-"),
        "shared output should contain crate1 CSS"
    );
    assert!(
        content.contains("wrapper-"),
        "shared output should contain crate2 CSS"
    );

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_shared_output_no_longer_errors() {
    // Previously, two crates sharing output_file would error.
    // Now it should succeed via concatenation.
    let binary = env!("CARGO_BIN_EXE_stylance");
    let crate1 = fixtures_dir().join("crate1");
    let crate2 = fixtures_dir().join("crate2");

    // Both fixture crates have output_file = "output.css" in their Cargo.toml.
    // With the old code this would error; now it should concatenate.
    let output = std::process::Command::new(binary)
        .arg(&crate1)
        .arg(&crate2)
        .output()
        .expect("failed to execute stylance binary");

    assert!(
        output.status.success(),
        "shared output_file should no longer error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------------------------------------------------------------------------
// CrateBuilder tests
// ---------------------------------------------------------------------------

#[test]
fn test_crate_builder_build_all() {
    let crate1 = fixtures_dir().join("crate1");
    let config = load_config(&crate1).unwrap();

    let mut builder = CrateBuilder::new(crate1, config);
    builder.build_all().unwrap();

    let output = builder.output().unwrap();
    assert_eq!(output.len(), 1);
    assert!(output[0].contents.contains("container-"));
}

#[test]
fn test_crate_builder_incremental_rebuild() {
    let tmpdir = std::env::temp_dir().join("stylance_test_incremental");
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(tmpdir.join("src")).unwrap();

    std::fs::write(tmpdir.join("src/a.module.css"), ".alpha { color: red; }").unwrap();
    std::fs::write(tmpdir.join("src/b.module.css"), ".beta { color: blue; }").unwrap();

    let config = Config {
        output_file: Some(tmpdir.join("out.css")),
        ..Config::default()
    };

    let mut builder = CrateBuilder::new(tmpdir.clone(), config);
    builder.build_all().unwrap();

    let output = builder.output().unwrap();
    assert_eq!(output.len(), 2);

    // Modify one file
    std::fs::write(tmpdir.join("src/a.module.css"), ".alpha { color: green; }").unwrap();

    let mut changed = std::collections::HashSet::new();
    changed.insert(tmpdir.join("src/a.module.css"));
    builder.rebuild_changed(&changed).unwrap();

    let output = builder.output().unwrap();
    assert_eq!(output.len(), 2);

    // The changed file should have the new content
    let alpha_result = output
        .iter()
        .find(|r| r.contents.contains("alpha-"))
        .unwrap();
    assert!(alpha_result.contents.contains("color: green"));

    // The unchanged file should still be present
    let beta_result = output
        .iter()
        .find(|r| r.contents.contains("beta-"))
        .unwrap();
    assert!(beta_result.contents.contains("color: blue"));

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_crate_builder_incremental_delete() {
    let tmpdir = std::env::temp_dir().join("stylance_test_incr_delete");
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(tmpdir.join("src")).unwrap();

    std::fs::write(tmpdir.join("src/a.module.css"), ".alpha { color: red; }").unwrap();
    std::fs::write(tmpdir.join("src/b.module.css"), ".beta { color: blue; }").unwrap();

    let config = Config::default();
    let mut builder = CrateBuilder::new(tmpdir.clone(), config);
    builder.build_all().unwrap();
    assert_eq!(builder.output().unwrap().len(), 2);

    // Delete one file
    std::fs::remove_file(tmpdir.join("src/b.module.css")).unwrap();

    let mut changed = std::collections::HashSet::new();
    changed.insert(tmpdir.join("src/b.module.css"));
    builder.rebuild_changed(&changed).unwrap();

    let output = builder.output().unwrap();
    assert_eq!(output.len(), 1);
    assert!(output[0].contents.contains("alpha-"));

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_crate_builder_incremental_create() {
    let tmpdir = std::env::temp_dir().join("stylance_test_incr_create");
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(tmpdir.join("src")).unwrap();

    std::fs::write(tmpdir.join("src/a.module.css"), ".alpha { color: red; }").unwrap();

    let config = Config::default();
    let mut builder = CrateBuilder::new(tmpdir.clone(), config);
    builder.build_all().unwrap();
    assert_eq!(builder.output().unwrap().len(), 1);

    // Create a new file
    std::fs::write(tmpdir.join("src/b.module.css"), ".beta { color: blue; }").unwrap();

    let mut changed = std::collections::HashSet::new();
    changed.insert(tmpdir.join("src/b.module.css"));
    builder.rebuild_changed(&changed).unwrap();

    let output = builder.output().unwrap();
    assert_eq!(output.len(), 2);

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_crate_builder_update_config() {
    let tmpdir = std::env::temp_dir().join("stylance_test_config_update");
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(tmpdir.join("src")).unwrap();

    std::fs::write(tmpdir.join("src/a.module.css"), ".alpha { color: red; }").unwrap();

    let config = Config {
        hash_len: Some(5),
        ..Config::default()
    };
    let mut builder = CrateBuilder::new(tmpdir.clone(), config);
    builder.build_all().unwrap();

    let output_before = builder.output().unwrap();
    let hash_before = &output_before[0].hash;
    assert_eq!(hash_before.len(), 5);

    // Update config with different hash_len
    let new_config = Config {
        hash_len: Some(10),
        ..Config::default()
    };
    builder.update_config(new_config).unwrap();

    let output_after = builder.output().unwrap();
    let hash_after = &output_after[0].hash;
    assert_eq!(hash_after.len(), 10);

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn test_crate_builder_from_results() {
    let crate1 = fixtures_dir().join("crate1");
    let config = load_config(&crate1).unwrap();

    let results = build_crate(&crate1, &config, |_| {}).unwrap();
    let builder = CrateBuilder::from_results(crate1, config, results);

    let output = builder.output().unwrap();
    assert_eq!(output.len(), 1);
    assert!(output[0].contents.contains("container-"));
}

// ---------------------------------------------------------------------------
// Content-aware output writing test
// ---------------------------------------------------------------------------

#[test]
fn test_content_aware_build_output() {
    let crate1 = fixtures_dir().join("crate1");
    let config = load_config(&crate1).unwrap();
    let results = build_crate(&crate1, &config, |_| {}).unwrap();

    let content1 = build_output_file_content(Path::new("output.css"), None, &[results.as_slice()]);
    let content2 = build_output_file_content(Path::new("output.css"), None, &[results.as_slice()]);

    // Same inputs should produce identical content
    assert_eq!(content1, content2);
}
