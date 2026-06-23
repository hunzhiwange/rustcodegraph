//! Tests for the CI/scripting fields `codegraph status --json` exposes (issue
//! #329): the `version`, `indexPath`, and `lastIndexed` fields, plus the
//! matching `CodeGraph::get_last_indexed_at()` library method.
//!
//! The CLI itself is exercised end-to-end against the built binary so the JSON
//! field names survive future refactors of the underlying plumbing.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_rustcodegraph");
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_millis() as i64
}

fn run_status_json(cwd: &Path) -> Value {
    let output = Command::new(BIN)
        .args(["status", "--json"])
        .current_dir(cwd)
        .env("RUSTCODEGRAPH_NO_DAEMON", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|err| panic!("failed to run codegraph status --json: {err}"));

    assert!(
        output.status.success(),
        "codegraph status --json failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // JSON mode prints exactly one line to stdout; be defensive about any stray
    // leading output by parsing the last non-empty line.
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let line = stdout
        .trim()
        .lines()
        .rfind(|line| !line.is_empty())
        .expect("status --json should print a JSON line");
    serde_json::from_str(line).unwrap_or_else(|err| panic!("status output should be JSON: {err}"))
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        for _ in 0..100 {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after Unix epoch")
                .as_nanos();
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("{prefix}{}-{unique}-{counter}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp dir {}: {err}", path.display()),
            }
        }
        panic!("failed to create unique temp dir with prefix {prefix}");
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

fn parse_iso_millis(value: &str) -> i64 {
    assert_eq!(value.len(), 24, "unexpected ISO timestamp shape: {value}");
    assert_eq!(&value[4..5], "-");
    assert_eq!(&value[7..8], "-");
    assert_eq!(&value[10..11], "T");
    assert_eq!(&value[13..14], ":");
    assert_eq!(&value[16..17], ":");
    assert_eq!(&value[19..20], ".");
    assert_eq!(&value[23..24], "Z");

    let year = value[0..4].parse::<i64>().expect("year should parse");
    let month = value[5..7].parse::<i64>().expect("month should parse");
    let day = value[8..10].parse::<i64>().expect("day should parse");
    let hour = value[11..13].parse::<i64>().expect("hour should parse");
    let minute = value[14..16].parse::<i64>().expect("minute should parse");
    let second = value[17..19].parse::<i64>().expect("second should parse");
    let millis = value[20..23].parse::<i64>().expect("millis should parse");

    let days = days_from_civil(year, month, day);
    days * 86_400_000 + (hour * 3_600 + minute * 60 + second) * 1_000 + millis
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

mod codegraph_status_json_ci_fields_329 {
    use super::*;

    #[test]
    fn get_last_indexed_at_is_null_before_indexing_and_a_recent_ms_timestamp_after() {
        let temp_dir = TempDir::new("codegraph-status-json-");
        let mut cg = CodeGraph::init_sync(temp_dir.path()).expect("CodeGraph should initialize");
        assert_eq!(cg.get_last_indexed_at(), None);

        fs::write(temp_dir.path().join("a.ts"), "export const x = 1;\n")
            .expect("fixture should be written");
        let before = now_ms();
        let _ = cg.index_all(IndexOptions::default());
        let after = now_ms();

        let last = cg.get_last_indexed_at();
        assert!(last.is_some(), "last indexed timestamp should be present");
        let last = last.expect("last indexed timestamp should be present");
        assert!(last >= before - 1_000, "{last} < {}", before - 1_000);
        assert!(last <= after + 1_000, "{last} > {}", after + 1_000);
        cg.close();
    }

    #[test]
    fn status_json_on_an_uninitialized_project_reports_version_index_path_and_last_indexed_null() {
        let temp_dir = TempDir::new("codegraph-status-json-");
        let out = run_status_json(temp_dir.path());

        assert_eq!(out["initialized"], false);
        assert_eq!(out["version"], PKG_VERSION);
        assert!(out["indexPath"].is_string());
        assert!(
            out["indexPath"]
                .as_str()
                .unwrap()
                .contains(".rustcodegraph"),
            "{}",
            out["indexPath"]
        );
        assert!(out["lastIndexed"].is_null());
    }

    #[test]
    fn status_json_on_an_indexed_project_reports_version_index_path_and_a_round_trippable_last_indexed()
     {
        let temp_dir = TempDir::new("codegraph-status-json-");
        fs::write(temp_dir.path().join("a.ts"), "export const x = 1;\n")
            .expect("fixture should be written");
        let before = now_ms();
        let mut cg = CodeGraph::init_sync(temp_dir.path()).expect("CodeGraph should initialize");
        let _ = cg.index_all(IndexOptions::default());
        let after = now_ms();
        cg.close();

        let out = run_status_json(temp_dir.path());
        assert_eq!(out["initialized"], true);
        assert_eq!(out["version"], PKG_VERSION);
        assert!(
            out["indexPath"]
                .as_str()
                .unwrap()
                .contains(".rustcodegraph"),
            "{}",
            out["indexPath"]
        );
        assert!(out["lastIndexed"].is_string());
        // ISO string that round-trips back into the index window.
        let ms = parse_iso_millis(out["lastIndexed"].as_str().unwrap());
        assert!(ms >= before - 1_000, "{ms} < {}", before - 1_000);
        assert!(ms <= after + 1_000, "{ms} > {}", after + 1_000);
    }
}
