pub(crate) use std::collections::BTreeMap;
pub(crate) use std::env;
use std::ffi::OsString;
pub(crate) use std::fs;
pub(crate) use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) use rustcodegraph::installer::index::{UninstallStatus, uninstall_targets};
pub(crate) use rustcodegraph::installer::targets::claude::cleanup_legacy_hooks;
pub(crate) use rustcodegraph::installer::targets::registry::{
    all_targets, get_target, resolve_target_flag,
};
pub(crate) use rustcodegraph::installer::targets::toml::{
    RemoveTomlAction, TomlValue, UpsertTomlAction, build_toml_table, remove_toml_table,
    upsert_toml_table,
};
pub(crate) use rustcodegraph::installer::targets::types::{
    AgentTarget, FileWrite, InstallOptions, Location, TargetId, WriteAction,
};
pub(crate) use serde_json::{Value, json};

pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

pub(crate) const LEGACY_BLOCK: &str = "<!-- CODEGRAPH_START -->\n## CodeGraph\n\nPrefer `codegraph_search` / `codegraph_callers` over grep.\n<!-- CODEGRAPH_END -->";

pub(crate) fn install_options(auto_allow: bool) -> InstallOptions {
    InstallOptions { auto_allow }
}

struct TempDir {
    root: PathBuf,
}

impl TempDir {
    pub(crate) fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let root =
            env::temp_dir().join(format!("cg-targets-{label}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&root).expect("failed to create temp dir");
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct EnvGuard {
    vars: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
    fn set_home(dir: &Path) -> Self {
        let vars = [
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "XDG_CONFIG_HOME",
            "HERMES_HOME",
        ]
        .into_iter()
        .map(|key| (key, env::var_os(key)))
        .collect::<Vec<_>>();
        set_env("HOME", dir);
        set_env("USERPROFILE", dir);
        set_env("APPDATA", dir.join(".config"));
        set_env("XDG_CONFIG_HOME", dir.join(".config"));
        remove_env("HERMES_HOME");
        Self { vars }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.vars {
            match value {
                Some(value) => set_env_os(key, value),
                None => remove_env(key),
            }
        }
    }
}

struct CwdGuard {
    original: PathBuf,
}

impl CwdGuard {
    fn chdir(dir: &Path) -> Self {
        let original = env::current_dir().expect("failed to read current dir");
        env::set_current_dir(dir).expect("failed to chdir to fixture cwd");
        Self { original }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = env::set_current_dir(&self.original);
    }
}

pub(crate) struct Fixture {
    _cwd_guard: CwdGuard,
    _env_guard: EnvGuard,
    tmp_home: TempDir,
    tmp_cwd: TempDir,
}

impl Fixture {
    pub(crate) fn new(label: &str) -> Self {
        let tmp_home = TempDir::new(&format!("{label}-home"));
        let tmp_cwd = TempDir::new(&format!("{label}-cwd"));
        let cwd_guard = CwdGuard::chdir(tmp_cwd.path());
        let env_guard = EnvGuard::set_home(tmp_home.path());
        Self {
            _cwd_guard: cwd_guard,
            _env_guard: env_guard,
            tmp_home,
            tmp_cwd,
        }
    }

    pub(crate) fn home(&self) -> &Path {
        self.tmp_home.path()
    }

    pub(crate) fn cwd(&self) -> &Path {
        self.tmp_cwd.path()
    }
}

pub(crate) fn with_fixture(label: &str, f: impl FnOnce(&Fixture)) {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let fixture = Fixture::new(label);
    f(&fixture);
}

pub(crate) fn set_env(key: &str, value: impl AsRef<Path>) {
    set_env_os(key, value.as_ref().as_os_str());
}

pub(crate) fn set_env_os(key: &str, value: impl AsRef<std::ffi::OsStr>) {
    unsafe {
        env::set_var(key, value);
    }
}

pub(crate) fn remove_env(key: &str) {
    unsafe {
        env::remove_var(key);
    }
}

pub(crate) fn read_text(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path).expect("expected text file to exist")
}

pub(crate) fn read_json(path: impl AsRef<Path>) -> Value {
    serde_json::from_str(&read_text(path)).expect("expected valid JSON")
}

pub(crate) fn write_json(path: impl AsRef<Path>, value: Value) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create JSON parent dir");
    }
    fs::write(
        path,
        serde_json::to_string_pretty(&value).expect("failed to serialize JSON") + "\n",
    )
    .expect("failed to write JSON fixture");
}

pub(crate) fn write_text(path: impl AsRef<Path>, text: &str) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create text parent dir");
    }
    fs::write(path, text).expect("failed to write text fixture");
}

pub(crate) fn list_all_files(dir: &Path) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).expect("failed to read fixture dir") {
        let entry = entry.expect("failed to read fixture entry");
        let full = entry.path();
        if full.is_dir() {
            out.extend(list_all_files(&full));
        } else {
            out.push(full);
        }
    }
    out
}

pub(crate) fn normalize(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().replace('\\', "/")
}

pub(crate) fn find_file<'a>(files: &'a [FileWrite], suffix: &str) -> Option<&'a FileWrite> {
    files
        .iter()
        .find(|file| normalize(&file.path).ends_with(suffix))
}

pub(crate) fn target(id: &str) -> Box<dyn AgentTarget> {
    get_target(id).unwrap_or_else(|| panic!("missing target {id}"))
}

pub(crate) fn supported_locations(target: &dyn AgentTarget) -> Vec<Location> {
    [Location::Global, Location::Local]
        .into_iter()
        .filter(|loc| target.supports_location(*loc))
        .collect()
}

pub(crate) fn all_target_ids() -> Vec<TargetId> {
    all_targets()
        .into_iter()
        .map(|target| target.id())
        .collect()
}
