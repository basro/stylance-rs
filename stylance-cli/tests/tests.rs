use std::{
    fs,
    path::{Path, PathBuf},
};

use tempfile::TempDir;
use walkdir::WalkDir;

/// Copies fixtures from a src folder into a dst folder
///
/// Renames files called `fixture.Cargo.toml` into `Cargo.toml`
///
/// This renaming is needed because folders containing Cargo.toml files are skipped
/// when cargo builds the package to publish in crates.io, this makes tests
/// fail when run from the sourcecode obtained from crates.io
fn copy_fixtures(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    fs::create_dir_all(dst)?;

    for entry in WalkDir::new(src) {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(src).unwrap();

        let target = match relative.file_name() {
            Some(name) if name == "fixture.Cargo.toml" => {
                dst.join(relative.with_file_name("Cargo.toml"))
            }
            _ => dst.join(relative),
        };

        if path.is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            fs::copy(path, &target)?;
        }
    }

    Ok(())
}

struct Setup {
    pub dir: TempDir,
    pub bin: PathBuf,
}

fn setup() -> Setup {
    let dir = TempDir::new().unwrap();
    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    copy_fixtures(fixtures_dir, dir.path()).unwrap();

    let bin = env!("CARGO_BIN_EXE_stylance");

    Setup {
        bin: bin.into(),
        dir,
    }
}

impl Setup {
    pub fn command(&self) -> std::process::Command {
        std::process::Command::new(&self.bin)
    }

    pub fn path<P>(&self, path_to_join: P) -> PathBuf
    where
        P: AsRef<Path>,
    {
        self.dir.path().join(path_to_join)
    }
}

fn assert_files_are_equal(file1: &Path, file2: &Path) {
    let a = fs::read_to_string(file1).unwrap();
    let b = fs::read_to_string(file2).unwrap();

    assert_eq!(a, b);
}

#[test]
fn test_multiple() {
    let setup = setup();

    // Verify the binary accepts multiple positional arguments
    let crate1 = setup.path("crate1");
    let crate2 = setup.path("crate2");

    let output = setup
        .command()
        .arg(&crate1)
        .arg(&crate2)
        .output()
        .expect("failed to execute stylance binary");

    assert!(
        output.status.success(),
        "stylance with multiple dirs failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_files_are_equal(
        &crate1.join("output.css"),
        &crate1.join("expected_output.css"),
    );

    assert_files_are_equal(
        &crate2.join("output.css"),
        &crate2.join("expected_output.css"),
    );
}

#[test]
fn test_multiple_output_file_cli_override() {
    let setup = setup();

    // Verify the binary accepts multiple positional arguments
    let crate1 = setup.path("crate1");
    let crate2 = setup.path("crate2");

    let output_path = setup.path("output.css");

    let output = setup
        .command()
        .arg(&crate1)
        .arg(&crate2)
        .arg("--output-file")
        .arg(&output_path)
        .output()
        .expect("failed to execute stylance binary");

    assert!(output.status.success());

    assert_files_are_equal(
        &output_path,
        &setup.path("crate1_2_combined_expected_output.css"),
    );
}

#[test]
fn test_workspace() {
    let setup = setup();

    // Verify the binary accepts multiple positional arguments
    let workspace = setup.path("workspace");

    let crate1 = workspace.join("crate1");
    let crate2 = workspace.join("crate2");

    let output = setup
        .command()
        .arg(&crate1)
        .arg(&crate2)
        .output()
        .expect("failed to execute stylance binary");

    assert!(output.status.success());

    assert_files_are_equal(
        &workspace.join("output.css"),
        &workspace.join("expected_output.css"),
    );
}
