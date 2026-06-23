//! npm package surface tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read_json(path: impl AsRef<Path>) -> Value {
    let text = fs::read_to_string(path.as_ref())
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.as_ref().display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|err| panic!("failed to parse {}: {err}", path.as_ref().display()))
}

fn string_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("expected JSON array")
        .iter()
        .map(|item| item.as_str().expect("expected JSON string"))
        .collect()
}

fn assert_metadata_only_root_package(pkg: &Value) {
    assert!(
        pkg.get("bin").is_none(),
        "root package should not define bin"
    );
    assert!(
        pkg.get("main").is_none(),
        "root package should not define main"
    );
    assert!(
        pkg.pointer("/exports/.").is_none(),
        "root package should not export the package root"
    );
    assert_eq!(
        pkg.pointer("/exports/.~1package.json"),
        Some(&json!("./package.json"))
    );

    let files = string_array(&pkg["files"]);
    assert!(files.contains(&"README.md"), "{files:?}");
    assert!(files.contains(&"LICENSE"), "{files:?}");
    assert!(
        !files.iter().any(|file| file.contains("npm-sdk")),
        "files should not include the removed JS SDK: {files:?}"
    );
}

#[test]
fn root_package_json_is_metadata_only() {
    let pkg = read_json(repo_root().join("package.json"));
    assert_metadata_only_root_package(&pkg);
}

#[test]
#[cfg_attr(windows, ignore = "pack-npm.sh is a POSIX release packaging script")]
fn pack_npm_generates_a_metadata_only_root_package() {
    let temp = TempDir::new("cg-pack-npm-surface");
    let release = temp.path().join("release");
    let archive_root = temp.path().join("archive");
    let binary_dir = archive_root.join("rustcodegraph-linux-x64").join("bin");
    fs::create_dir_all(&binary_dir)
        .unwrap_or_else(|err| panic!("failed to create {}: {err}", binary_dir.display()));
    fs::write(
        binary_dir.join("rustcodegraph"),
        "#!/bin/sh\necho fake-rustcodegraph\n",
    )
    .unwrap_or_else(|err| panic!("failed to write fake binary: {err}"));

    fs::create_dir_all(&release)
        .unwrap_or_else(|err| panic!("failed to create {}: {err}", release.display()));
    let archive = release.join("rustcodegraph-linux-x64.tar.gz");
    let tar = Command::new("tar")
        .args([
            "-czf",
            archive.to_string_lossy().as_ref(),
            "-C",
            archive_root.to_string_lossy().as_ref(),
            "rustcodegraph-linux-x64",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|err| panic!("failed to create fake release archive: {err}"));
    assert!(
        tar.status.success(),
        "tar failed with status {:?}\nstderr:\n{}",
        tar.status.code(),
        String::from_utf8_lossy(&tar.stderr)
    );

    let output = Command::new("bash")
        .arg(repo_root().join("scripts").join("pack-npm.sh"))
        .arg("9.9.9-test")
        .env("RUSTCODEGRAPH_RELEASE_DIR", &release)
        .current_dir(repo_root())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|err| panic!("failed to run pack-npm.sh: {err}"));
    assert!(
        output.status.success(),
        "pack-npm.sh failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let npm_root = release.join("npm").join("main");
    let main_pkg = read_json(npm_root.join("package.json"));
    assert_metadata_only_root_package(&main_pkg);
    assert_eq!(
        main_pkg.pointer("/optionalDependencies/@colbymchenry~1rustcodegraph-linux-x64"),
        Some(&json!("9.9.9-test"))
    );
    assert!(
        !npm_root.join("npm-sdk.js").exists(),
        "packaging should not copy the removed JS SDK"
    );
}

#[test]
#[cfg_attr(windows, ignore = "pack-npm.sh is a POSIX release packaging script")]
fn pack_npm_accepts_cargo_dist_root_binary_archives() {
    let temp = TempDir::new("cg-pack-npm-cargo-dist");
    let release = temp.path().join("release");
    let archive_root = temp.path().join("archive");
    let binary_dir = archive_root.join("rustcodegraph-aarch64-apple-darwin");
    fs::create_dir_all(&binary_dir)
        .unwrap_or_else(|err| panic!("failed to create {}: {err}", binary_dir.display()));
    fs::write(
        binary_dir.join("rustcodegraph"),
        "#!/bin/sh\necho fake-rustcodegraph\n",
    )
    .unwrap_or_else(|err| panic!("failed to write fake binary: {err}"));

    fs::create_dir_all(&release)
        .unwrap_or_else(|err| panic!("failed to create {}: {err}", release.display()));
    let archive = release.join("rustcodegraph-aarch64-apple-darwin.tar.xz");
    let tar = Command::new("tar")
        .args([
            "-cJf",
            archive.to_string_lossy().as_ref(),
            "-C",
            archive_root.to_string_lossy().as_ref(),
            "rustcodegraph-aarch64-apple-darwin",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|err| panic!("failed to create fake cargo-dist archive: {err}"));
    assert!(
        tar.status.success(),
        "tar failed with status {:?}\nstderr:\n{}",
        tar.status.code(),
        String::from_utf8_lossy(&tar.stderr)
    );

    let output = Command::new("bash")
        .arg(repo_root().join("scripts").join("pack-npm.sh"))
        .arg("9.9.9-test")
        .env("RUSTCODEGRAPH_RELEASE_DIR", &release)
        .current_dir(repo_root())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|err| panic!("failed to run pack-npm.sh: {err}"));
    assert!(
        output.status.success(),
        "pack-npm.sh failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let pkg_root = release.join("npm").join("rustcodegraph-darwin-arm64");
    assert!(
        pkg_root.join("bin").join("rustcodegraph").exists(),
        "platform package should contain the copied cargo-dist binary"
    );
    let platform_pkg = read_json(pkg_root.join("package.json"));
    assert_eq!(
        platform_pkg.pointer("/name"),
        Some(&json!("@colbymchenry/rustcodegraph-darwin-arm64"))
    );
    assert_eq!(platform_pkg.pointer("/os/0"), Some(&json!("darwin")));
    assert_eq!(platform_pkg.pointer("/cpu/0"), Some(&json!("arm64")));

    let main_pkg = read_json(release.join("npm").join("main").join("package.json"));
    assert_eq!(
        main_pkg.pointer("/optionalDependencies/@colbymchenry~1rustcodegraph-darwin-arm64"),
        Some(&json!("9.9.9-test"))
    );
}

#[test]
#[cfg_attr(windows, ignore = "pack-npm.sh is a POSIX release packaging script")]
fn pack_npm_rejects_an_archive_that_only_contains_the_old_codegraph_binary() {
    let temp = TempDir::new("cg-pack-npm-old-binary");
    let release = temp.path().join("release");
    let archive_root = temp.path().join("archive");
    let binary_dir = archive_root.join("rustcodegraph-linux-x64").join("bin");
    fs::create_dir_all(&binary_dir)
        .unwrap_or_else(|err| panic!("failed to create {}: {err}", binary_dir.display()));
    fs::write(
        binary_dir.join("codegraph"),
        "#!/bin/sh\necho old-codegraph\n",
    )
    .unwrap_or_else(|err| panic!("failed to write old fake binary: {err}"));

    fs::create_dir_all(&release)
        .unwrap_or_else(|err| panic!("failed to create {}: {err}", release.display()));
    let archive = release.join("rustcodegraph-linux-x64.tar.gz");
    let tar = Command::new("tar")
        .args([
            "-czf",
            archive.to_string_lossy().as_ref(),
            "-C",
            archive_root.to_string_lossy().as_ref(),
            "rustcodegraph-linux-x64",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|err| panic!("failed to create fake release archive: {err}"));
    assert!(
        tar.status.success(),
        "tar failed with status {:?}\nstderr:\n{}",
        tar.status.code(),
        String::from_utf8_lossy(&tar.stderr)
    );

    let output = Command::new("bash")
        .arg(repo_root().join("scripts").join("pack-npm.sh"))
        .arg("9.9.9-test")
        .env("RUSTCODEGRAPH_RELEASE_DIR", &release)
        .current_dir(repo_root())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|err| panic!("failed to run pack-npm.sh: {err}"));
    assert!(
        !output.status.success(),
        "pack-npm.sh should reject old codegraph binaries\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("did not contain rustcodegraph binary"),
        "unexpected stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        for _ in 0..100 {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after Unix epoch")
                .as_nanos();
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("{label}-{}-{unique}-{counter}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp dir {}: {err}", path.display()),
            }
        }
        panic!("failed to create a unique temp dir for {label}");
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
