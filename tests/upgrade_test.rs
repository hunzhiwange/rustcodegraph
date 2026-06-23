use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use rusqlite::{Connection, params};
use rustcodegraph::directory::get_code_graph_dir;
use rustcodegraph::extraction::extraction_version::EXTRACTION_VERSION;
use rustcodegraph::upgrade::index::{
    BundleOs, DetectInput, InstallMethod, NPM_PACKAGE, NpmScope, UpgradeDeps, UpgradeOptions,
    build_windows_upgrade_script, compare_versions, derive_install_dir, detect_install_method,
    is_update_available, normalize_version, parse_latest_tag_from_location, parse_semver,
    reindex_advisory, run_upgrade, strip_v,
};
use rustcodegraph::{CodeGraph, IndexOptions, InitOptions};

// ---------------------------------------------------------------------------
// detectInstallMethod - structural detection from the running file's path
// ---------------------------------------------------------------------------

fn bundle_exists(present: BTreeSet<String>) -> impl Fn(&str) -> bool {
    move |path| present.contains(&path.replace('\\', "/"))
}

mod detect_install_method {
    use super::*;

    #[test]
    fn detects_a_unix_rust_bundle_and_derives_the_install_dir_from_the_versions_layout() {
        let root = "/home/u/.rustcodegraph/versions/v0.9.9";
        let filename = format!("{root}/bin/rustcodegraph");
        let present = BTreeSet::from([
            format!("{root}/bin/rustcodegraph"),
            format!("{root}/package.json"),
            "/home/u/.rustcodegraph".to_owned(),
        ]);
        let method = detect_install_method(DetectInput {
            filename: &filename,
            platform: "linux",
            cwd: "/home/u/project",
            exists: bundle_exists(present),
        });
        assert_eq!(
            method,
            InstallMethod::Bundle {
                os: BundleOs::Unix,
                bundle_root: root.to_owned(),
                install_dir: Some("/home/u/.rustcodegraph".to_owned()),
            }
        );
    }

    #[test]
    fn detects_a_windows_rust_bundle_and_derives_the_install_dir_from_current() {
        let root = "C:/Users/u/AppData/Local/rustcodegraph/current";
        let filename = format!("{root}/bin/rustcodegraph.exe");
        let present = BTreeSet::from([
            format!("{root}/bin/rustcodegraph.exe"),
            format!("{root}/package.json"),
        ]);
        let method = detect_install_method(DetectInput {
            filename: &filename,
            platform: "win32",
            cwd: "C:/Users/u/project",
            exists: bundle_exists(present),
        });
        let InstallMethod::Bundle {
            os, install_dir, ..
        } = method
        else {
            panic!("expected bundle, got {method:?}");
        };
        assert_eq!(os, BundleOs::Windows);
        assert_eq!(
            install_dir.map(|dir| dir.replace('\\', "/")),
            Some("C:/Users/u/AppData/Local/rustcodegraph".to_owned())
        );
    }

    #[test]
    fn does_not_detect_a_legacy_node_bundle_from_the_codegraph_project() {
        let root = "/home/u/.rustcodegraph/versions/v0.9.8";
        let filename = format!("{root}/lib/dist/bin/codegraph.js");
        let present = BTreeSet::from([
            format!("{root}/node"),
            format!("{root}/bin/codegraph"),
            "/home/u/.rustcodegraph".to_owned(),
        ]);
        let method = detect_install_method(DetectInput {
            filename: &filename,
            platform: "linux",
            cwd: "/home/u/project",
            exists: bundle_exists(present),
        });
        assert!(matches!(method, InstallMethod::Unknown { .. }));
    }

    #[test]
    fn detects_a_global_npm_install() {
        let filename =
            "/usr/local/lib/node_modules/rustcodegraph-linux-x64/bin/rustcodegraph";
        let method = detect_install_method(DetectInput {
            filename,
            platform: "linux",
            cwd: "/home/u/project",
            exists: |_| false,
        });
        assert_eq!(
            method,
            InstallMethod::Npm {
                scope: NpmScope::Global
            }
        );
    }

    #[test]
    fn detects_a_global_cargo_dist_npm_install() {
        let filename = "/usr/local/lib/node_modules/rustcodegraph/node_modules/.bin_real/rustcodegraph";
        let method = detect_install_method(DetectInput {
            filename,
            platform: "linux",
            cwd: "/home/u/project",
            exists: |_| false,
        });
        assert_eq!(
            method,
            InstallMethod::Npm {
                scope: NpmScope::Global
            }
        );
    }

    #[test]
    fn detects_a_local_project_npm_install_as_local() {
        let cwd = "/home/u/project";
        let filename =
            format!("{cwd}/node_modules/rustcodegraph-linux-x64/bin/rustcodegraph");
        let method = detect_install_method(DetectInput {
            filename: &filename,
            platform: "linux",
            cwd,
            exists: |_| false,
        });
        assert_eq!(
            method,
            InstallMethod::Npm {
                scope: NpmScope::Local
            }
        );
    }

    #[test]
    fn detects_a_local_cargo_dist_npm_install_as_local() {
        let cwd = "/home/u/project";
        let filename = format!(
            "{cwd}/node_modules/rustcodegraph/node_modules/.bin_real/rustcodegraph"
        );
        let method = detect_install_method(DetectInput {
            filename: &filename,
            platform: "linux",
            cwd,
            exists: |_| false,
        });
        assert_eq!(
            method,
            InstallMethod::Npm {
                scope: NpmScope::Local
            }
        );
    }

    #[test]
    fn detects_an_npx_run_from_the_npx_cache() {
        let filename = "/home/u/.npm/_npx/abc123/node_modules/rustcodegraph-linux-x64/bin/rustcodegraph";
        let method = detect_install_method(DetectInput {
            filename,
            platform: "linux",
            cwd: "/home/u",
            exists: |_| false,
        });
        assert_eq!(method, InstallMethod::Npx);
    }

    #[test]
    fn detects_a_cargo_dist_npx_run_from_the_npx_cache() {
        let filename = "/home/u/.npm/_npx/abc123/node_modules/rustcodegraph/node_modules/.bin_real/rustcodegraph";
        let method = detect_install_method(DetectInput {
            filename,
            platform: "linux",
            cwd: "/home/u",
            exists: |_| false,
        });
        assert_eq!(method, InstallMethod::Npx);
    }

    #[test]
    fn does_not_detect_the_old_codegraph_npm_package() {
        let filename =
            "/usr/local/lib/node_modules/rustcodegraph-linux-x64/bin/codegraph";
        let method = detect_install_method(DetectInput {
            filename,
            platform: "linux",
            cwd: "/home/u/project",
            exists: |_| false,
        });
        assert!(matches!(method, InstallMethod::Unknown { .. }));
    }

    #[test]
    fn detects_a_source_checkout_via_sibling_package_json_and_git() {
        let repo = "/home/u/dev/rustcodegraph";
        let filename = format!("{repo}/target/release/rustcodegraph");
        let present = BTreeSet::from([format!("{repo}/Cargo.toml"), format!("{repo}/.git")]);
        let method = detect_install_method(DetectInput {
            filename: &filename,
            platform: "darwin",
            cwd: repo,
            exists: bundle_exists(present),
        });
        assert_eq!(
            method,
            InstallMethod::Source {
                root: repo.to_owned()
            }
        );
    }

    #[test]
    fn returns_unknown_for_an_unrecognized_layout() {
        let method = detect_install_method(DetectInput {
            filename: "/opt/weird/place/rustcodegraph.js",
            platform: "linux",
            cwd: "/tmp",
            exists: |_| false,
        });
        assert!(matches!(method, InstallMethod::Unknown { .. }));
    }
}

mod derive_install_dir {
    use super::*;

    #[test]
    fn unix_returns_the_dir_above_versions() {
        assert_eq!(
            derive_install_dir(
                "/a/b/.rustcodegraph/versions/v1.2.3",
                BundleOs::Unix,
                |_| true
            ),
            Some("/a/b/.rustcodegraph".to_owned())
        );
    }

    #[test]
    fn unix_null_when_not_under_versions() {
        assert_eq!(
            derive_install_dir("/a/b/somewhere", BundleOs::Unix, |_| true),
            None
        );
    }

    #[test]
    fn windows_returns_the_parent_of_current() {
        assert_eq!(
            derive_install_dir("C:/x/rustcodegraph/current", BundleOs::Windows, |_| true)
                .map(|dir| dir.replace('\\', "/")),
            Some("C:/x/rustcodegraph".to_owned())
        );
    }

    #[test]
    fn windows_null_when_basename_is_not_current() {
        assert_eq!(
            derive_install_dir("C:/x/rustcodegraph/v1", BundleOs::Windows, |_| true),
            None
        );
    }
}

// ---------------------------------------------------------------------------
// version helpers
// ---------------------------------------------------------------------------

mod version_helpers {
    use super::*;

    #[test]
    fn parse_semver_handles_v_prefix_and_prerelease() {
        let parsed = parse_semver("v1.2.3").expect("v-prefixed semver should parse");
        assert_eq!(parsed.major, 1);
        assert_eq!(parsed.minor, 2);
        assert_eq!(parsed.patch, 3);
        assert_eq!(parsed.pre, None);

        let parsed = parse_semver("1.2.3-rc.1").expect("prerelease semver should parse");
        assert_eq!(parsed.major, 1);
        assert_eq!(parsed.minor, 2);
        assert_eq!(parsed.patch, 3);
        assert_eq!(parsed.pre, Some("rc.1".to_owned()));

        assert!(parse_semver("not-a-version").is_none());
    }

    #[test]
    fn compare_versions_orders_correctly_incl_prerelease_less_than_release() {
        assert!(compare_versions("1.0.1", "1.0.0").unwrap() > 0);
        assert!(compare_versions("1.0.0", "1.1.0").unwrap() < 0);
        assert_eq!(compare_versions("v2.0.0", "2.0.0").unwrap(), 0);
        assert!(compare_versions("1.0.0-rc.1", "1.0.0").unwrap() < 0);
    }

    #[test]
    fn is_update_available_compares_and_falls_back_to_string_inequality_for_unparseable() {
        assert!(is_update_available("0.9.8", "0.9.9"));
        assert!(!is_update_available("0.9.9", "0.9.9"));
        assert!(!is_update_available("0.9.9", "0.9.8"));
        assert!(is_update_available("0.0.0-unknown", "0.9.9"));
    }

    #[test]
    fn normalize_version_strip_v_round_trip() {
        assert_eq!(normalize_version("0.9.9"), "v0.9.9");
        assert_eq!(normalize_version("v0.9.9"), "v0.9.9");
        assert_eq!(strip_v("v0.9.9"), "0.9.9");
        assert_eq!(strip_v("0.9.9"), "0.9.9");
    }

    #[test]
    fn parse_latest_tag_from_location_extracts_the_tag_from_a_releases_redirect() {
        assert_eq!(
            parse_latest_tag_from_location(Some(
                "https://github.com/hunzhiwange/rustcodegraph/releases/tag/v0.9.9"
            )),
            Some("v0.9.9".to_owned())
        );
        assert_eq!(
            parse_latest_tag_from_location(Some(
                "https://github.com/o/r/releases/tag/v1.2.3?foo=bar"
            )),
            Some("v1.2.3".to_owned())
        );
        assert_eq!(parse_latest_tag_from_location(None), None);
        assert_eq!(
            parse_latest_tag_from_location(Some("https://github.com/o/r/releases")),
            None
        );
    }

    #[test]
    fn reindex_advisory_mentions_the_refresh_commands() {
        let advisory = reindex_advisory();
        assert!(advisory.contains("rustcodegraph sync"));
        assert!(advisory.contains("rustcodegraph index -f"));
    }

    #[test]
    fn build_windows_upgrade_script_targets_the_right_asset_per_arch_and_renames_not_deletes_the_exe()
     {
        let arm = build_windows_upgrade_script(r"C:\cg\current", "v1.2.3", "arm64");
        assert!(arm.contains("github.com/hunzhiwange/rustcodegraph/releases/download/v1.2.3/"));
        assert!(arm.contains("releases/download/v1.2.3/rustcodegraph-aarch64-pc-windows-msvc.zip"));
        assert!(arm.contains(r"$dest='C:\cg\current'"));
        assert!(arm.contains("downloaded archive did not contain bin\\rustcodegraph.exe"));
        assert!(arm.contains("Rename-Item"));
        assert!(arm.contains("rustcodegraph.exe.old-"));
        assert!(!arm.contains("Join-Path $dest 'rustcodegraph.exe'"));
        let remove_dest = Regex::new(r"Remove-Item[^;]*\$dest'?\s*;").unwrap();
        assert!(!remove_dest.is_match(&arm));

        let x64 = build_windows_upgrade_script(r"C:\cg\current", "v1.2.3", "x64");
        assert!(x64.contains("rustcodegraph-x86_64-pc-windows-msvc.zip"));
    }
}

// ---------------------------------------------------------------------------
// runUpgrade orchestration - mocked side-effects
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunCall {
    cmd: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

#[derive(Debug, Default)]
struct Calls {
    runs: Vec<RunCall>,
    logs: Vec<String>,
    errors: Vec<String>,
}

fn make_deps(
    method: InstallMethod,
    current_version: &str,
) -> (UpgradeDeps<'static>, Rc<RefCell<Calls>>) {
    make_deps_with(method, current_version, 0, |_| true, "linux")
}

fn make_deps_with<H>(
    method: InstallMethod,
    current_version: &str,
    run_exit: i32,
    has_command: H,
    platform: &str,
) -> (UpgradeDeps<'static>, Rc<RefCell<Calls>>)
where
    H: Fn(&str) -> bool + 'static,
{
    let calls = Rc::new(RefCell::new(Calls::default()));

    let run_calls = Rc::clone(&calls);
    let log_calls = Rc::clone(&calls);
    let warn_calls = Rc::clone(&calls);
    let error_calls = Rc::clone(&calls);

    let deps = UpgradeDeps {
        current_version: current_version.to_owned(),
        method,
        resolve_latest: Box::new(|| Ok("v0.9.9".to_owned())),
        run: Box::new(move |cmd, args, env| {
            run_calls.borrow_mut().runs.push(RunCall {
                cmd: cmd.to_owned(),
                args: args.to_vec(),
                env: env.to_vec(),
            });
            run_exit
        }),
        has_command: Box::new(has_command),
        log: Box::new(move |message| log_calls.borrow_mut().logs.push(message.to_owned())),
        warn: Box::new(move |message| warn_calls.borrow_mut().logs.push(message.to_owned())),
        error: Box::new(move |message| error_calls.borrow_mut().errors.push(message.to_owned())),
        platform: platform.to_owned(),
    };

    (deps, calls)
}

fn joined_logs(calls: &Rc<RefCell<Calls>>) -> String {
    calls.borrow().logs.join("\n")
}

fn joined_errors(calls: &Rc<RefCell<Calls>>) -> String {
    calls.borrow().errors.join("\n")
}

fn env_value(call: &RunCall, key: &str) -> Option<String> {
    call.env
        .iter()
        .find_map(|(candidate, value)| (candidate == key).then(|| value.clone()))
}

fn decode_encoded_command(args: &[String]) -> String {
    let index = args
        .iter()
        .position(|arg| arg == "-EncodedCommand")
        .expect("no -EncodedCommand in args");
    let bytes = base64_decode(&args[index + 1]);
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units).expect("encoded PowerShell payload should be UTF-16LE")
}

fn base64_decode(input: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' => continue,
            _ => panic!("invalid base64 byte {byte}"),
        } as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    output
}

mod run_upgrade_orchestration {
    use super::*;

    #[test]
    fn does_nothing_when_already_up_to_date() {
        let (deps, calls) = make_deps(
            InstallMethod::Npm {
                scope: NpmScope::Global,
            },
            "0.9.9",
        );
        let code = run_upgrade(UpgradeOptions::default(), deps);
        assert_eq!(code, 0);
        assert_eq!(calls.borrow().runs.len(), 0);
        assert!(joined_logs(&calls).to_lowercase().contains("up to date"));
    }

    #[test]
    fn check_reports_an_available_update_without_running_anything() {
        let (deps, calls) = make_deps(
            InstallMethod::Npm {
                scope: NpmScope::Global,
            },
            "0.9.8",
        );
        let code = run_upgrade(
            UpgradeOptions {
                check: true,
                ..UpgradeOptions::default()
            },
            deps,
        );
        assert_eq!(code, 0);
        assert_eq!(calls.borrow().runs.len(), 0);
        assert!(
            joined_logs(&calls)
                .to_lowercase()
                .contains("update is available")
        );
    }

    #[test]
    fn unix_bundle_runs_the_installer_via_sh_with_the_derived_install_dir() {
        let (deps, calls) = make_deps(
            InstallMethod::Bundle {
                os: BundleOs::Unix,
                bundle_root: "/h/.rustcodegraph/versions/v0.9.8".to_owned(),
                install_dir: Some("/h/.rustcodegraph".to_owned()),
            },
            "0.9.8",
        );
        let code = run_upgrade(UpgradeOptions::default(), deps);
        assert_eq!(code, 0);
        let calls_ref = calls.borrow();
        assert_eq!(calls_ref.runs.len(), 1);
        assert_eq!(calls_ref.runs[0].cmd, "sh");
        assert_eq!(calls_ref.runs[0].args[0], "-c");
        assert!(calls_ref.runs[0].args[1].contains("curl -fsSL"));
        assert!(calls_ref.runs[0].args[1].contains("| sh"));
        assert_eq!(
            env_value(&calls_ref.runs[0], "RUSTCODEGRAPH_INSTALL_DIR"),
            Some("/h/.rustcodegraph".to_owned())
        );
        drop(calls_ref);
        assert!(joined_logs(&calls).contains("rustcodegraph sync"));
    }

    #[test]
    fn unix_bundle_falls_back_to_wget_and_errors_when_neither_downloader_exists() {
        let (deps, calls) = make_deps_with(
            InstallMethod::Bundle {
                os: BundleOs::Unix,
                bundle_root: "/h/.rustcodegraph/versions/v0.9.8".to_owned(),
                install_dir: None,
            },
            "0.9.8",
            0,
            |_| false,
            "linux",
        );
        let code = run_upgrade(UpgradeOptions::default(), deps);
        assert_eq!(code, 1);
        assert_eq!(calls.borrow().runs.len(), 0);
        assert!(
            joined_errors(&calls)
                .to_lowercase()
                .contains("curl nor wget")
        );
    }

    #[test]
    fn windows_bundle_runs_a_synchronous_in_place_rename_and_extract_powershell_upgrade() {
        let (deps, calls) = make_deps_with(
            InstallMethod::Bundle {
                os: BundleOs::Windows,
                bundle_root: "C:/x/rustcodegraph/current".to_owned(),
                install_dir: Some("C:/x/rustcodegraph".to_owned()),
            },
            "0.9.8",
            0,
            |_| true,
            "win32",
        );
        let code = run_upgrade(UpgradeOptions::default(), deps);
        assert_eq!(code, 0);
        let calls_ref = calls.borrow();
        assert_eq!(calls_ref.runs.len(), 1);
        assert_eq!(calls_ref.runs[0].cmd, "powershell.exe");
        let decoded = decode_encoded_command(&calls_ref.runs[0].args);
        assert!(decoded.contains("releases/download/v0.9.9/rustcodegraph-"));
        assert!(decoded.contains("downloaded archive did not contain bin\\rustcodegraph.exe"));
        assert!(decoded.contains("Rename-Item"));
        assert!(decoded.contains("rustcodegraph.exe.old-"));
        assert!(decoded.contains("Copy-Item"));
        assert!(!decoded.contains("Join-Path $dest 'rustcodegraph.exe'"));
    }

    #[test]
    fn windows_bundle_a_non_zero_installer_exit_is_a_failure() {
        let (deps, calls) = make_deps_with(
            InstallMethod::Bundle {
                os: BundleOs::Windows,
                bundle_root: "C:/x/rustcodegraph/current".to_owned(),
                install_dir: Some("C:/x/rustcodegraph".to_owned()),
            },
            "0.9.8",
            1,
            |_| true,
            "win32",
        );
        let code = run_upgrade(UpgradeOptions::default(), deps);
        assert_eq!(code, 1);
        assert!(
            joined_errors(&calls)
                .to_lowercase()
                .contains("exited with code")
        );
    }

    #[test]
    fn npm_global_shells_out_to_npm_install_g_pkg_latest() {
        let (deps, calls) = make_deps(
            InstallMethod::Npm {
                scope: NpmScope::Global,
            },
            "0.9.8",
        );
        let code = run_upgrade(UpgradeOptions::default(), deps);
        assert_eq!(code, 0);
        let calls_ref = calls.borrow();
        assert_eq!(calls_ref.runs[0].cmd, "npm");
        assert_eq!(
            calls_ref.runs[0].args,
            vec![
                "install".to_owned(),
                "-g".to_owned(),
                format!("{NPM_PACKAGE}@latest")
            ]
        );
    }

    #[test]
    fn npm_on_win32_uses_npm_cmd() {
        let (deps, calls) = make_deps_with(
            InstallMethod::Npm {
                scope: NpmScope::Global,
            },
            "0.9.8",
            0,
            |_| true,
            "win32",
        );
        let code = run_upgrade(UpgradeOptions::default(), deps);
        assert_eq!(code, 0);
        assert_eq!(calls.borrow().runs[0].cmd, "npm.cmd");
    }

    #[test]
    fn npm_a_pinned_version_is_passed_through_as_at_version() {
        let (deps, calls) = make_deps(
            InstallMethod::Npm {
                scope: NpmScope::Global,
            },
            "0.9.9",
        );
        let code = run_upgrade(
            UpgradeOptions {
                version: Some("0.9.8".to_owned()),
                ..UpgradeOptions::default()
            },
            deps,
        );
        assert_eq!(code, 0);
        assert_eq!(
            calls.borrow().runs[0].args,
            vec![
                "install".to_owned(),
                "-g".to_owned(),
                format!("{NPM_PACKAGE}@0.9.8")
            ]
        );
    }

    #[test]
    fn npm_surfaces_a_non_zero_exit_as_failure() {
        let (deps, calls) = make_deps_with(
            InstallMethod::Npm {
                scope: NpmScope::Global,
            },
            "0.9.8",
            1,
            |_| true,
            "linux",
        );
        let code = run_upgrade(UpgradeOptions::default(), deps);
        assert_eq!(code, 1);
        assert!(joined_errors(&calls).to_lowercase().contains("npm exited"));
    }

    #[test]
    fn npx_nothing_to_upgrade() {
        let (deps, calls) = make_deps(InstallMethod::Npx, "0.9.8");
        let code = run_upgrade(UpgradeOptions::default(), deps);
        assert_eq!(code, 0);
        assert_eq!(calls.borrow().runs.len(), 0);
        assert!(
            joined_logs(&calls)
                .to_lowercase()
                .contains("nothing to upgrade")
        );
    }

    #[test]
    fn source_tells_the_user_to_git_pull_runs_nothing() {
        let (deps, calls) = make_deps(
            InstallMethod::Source {
                root: "/dev/rustcodegraph".to_owned(),
            },
            "0.9.8",
        );
        let code = run_upgrade(UpgradeOptions::default(), deps);
        assert_eq!(code, 0);
        assert_eq!(calls.borrow().runs.len(), 0);
        assert!(joined_logs(&calls).contains("git pull"));
        assert!(joined_logs(&calls).contains("cargo build --release"));
    }
}

// ---------------------------------------------------------------------------
// Re-index staleness - real index, real metadata stamp
// ---------------------------------------------------------------------------

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", path.display()));
        Self { path }
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

fn set_index_metadata(project_root: &Path, key: &str, value: &str) {
    let db_path = get_code_graph_dir(project_root).join("rustcodegraph.db");
    let conn = Connection::open(&db_path)
        .unwrap_or_else(|err| panic!("failed to open {}: {err}", db_path.display()));
    conn.execute(
        "INSERT INTO project_metadata (key, value, updated_at) VALUES (?, ?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, 0i64],
    )
    .unwrap_or_else(|err| panic!("failed to set metadata {key}: {err}"));
}

mod index_extraction_version_stamp_is_index_stale {
    use super::*;

    #[test]
    fn stamps_the_current_extraction_version_on_full_index_and_is_not_stale() {
        let dir = TempDir::new("cg-upgrade-stamp");
        fs::write(
            dir.path().join("a.ts"),
            "export function hello() { return 1; }\n",
        )
        .expect("fixture should be written");

        let mut codegraph = CodeGraph::init(dir.path(), InitOptions { index: false })
            .expect("CodeGraph should initialize");
        assert!(!codegraph.is_index_stale());

        let result = codegraph.index_all(IndexOptions::default());
        assert!(result.success, "{result:?}");
        let info = codegraph.get_index_build_info();
        assert_eq!(info.extraction_version, Some(EXTRACTION_VERSION as u64));
        assert!(info.version.is_some());
        assert!(!codegraph.is_index_stale());
        codegraph.close();
    }

    #[test]
    fn flags_an_index_stamped_by_an_older_extraction_version_as_stale() {
        let dir = TempDir::new("cg-upgrade-stamp");
        fs::write(
            dir.path().join("a.ts"),
            "export function hello() { return 1; }\n",
        )
        .expect("fixture should be written");

        let mut codegraph = CodeGraph::init(dir.path(), InitOptions { index: false })
            .expect("CodeGraph should initialize");
        let result = codegraph.index_all(IndexOptions::default());
        assert!(result.success, "{result:?}");

        set_index_metadata(
            dir.path(),
            "indexed_with_extraction_version",
            &(EXTRACTION_VERSION - 1).to_string(),
        );
        assert!(codegraph.is_index_stale());
        codegraph.close();
    }
}
