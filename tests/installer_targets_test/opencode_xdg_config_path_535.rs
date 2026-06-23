use super::common::*;

struct XdgFixture {
    fixture: Fixture,
    app_data_dir: PathBuf,
}

impl XdgFixture {
    fn new() -> Self {
        let fixture = Fixture::new("opencode-xdg");
        let app_data_dir = fixture.home().join("AppData").join("Roaming");
        set_env("APPDATA", &app_data_dir);
        remove_env("XDG_CONFIG_HOME");
        Self {
            fixture,
            app_data_dir,
        }
    }

    fn xdg_config_file(&self) -> PathBuf {
        self.fixture
            .home()
            .join(".config")
            .join("opencode")
            .join("opencode.jsonc")
    }

    fn legacy_dir(&self) -> PathBuf {
        self.app_data_dir.join("opencode")
    }

    fn in_legacy_dir(&self, path: &str) -> bool {
        Path::new(path).starts_with(self.legacy_dir())
    }
}

fn with_xdg_fixture(f: impl FnOnce(&XdgFixture)) {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let fixture = XdgFixture::new();
    f(&fixture);
}

#[test]
fn global_install_writes_to_home_config_opencode_never_appdata_535() {
    with_xdg_fixture(|fixture| {
        let opencode = target("opencode");
        let result = opencode.install(Location::Global, install_options(true));
        let written = result
            .files
            .iter()
            .find(|file| file.path.ends_with("opencode.jsonc"))
            .unwrap();
        assert_eq!(written.action, WriteAction::Created);
        assert_eq!(Path::new(&written.path), fixture.xdg_config_file());
        assert!(fixture.xdg_config_file().exists());
        assert!(!fixture.legacy_dir().exists());
    });
}

#[test]
fn greenfield_targets_home_config_opencode_even_when_dir_does_not_exist_yet_535() {
    with_xdg_fixture(|fixture| {
        assert!(
            !fixture
                .fixture
                .home()
                .join(".config")
                .join("opencode")
                .exists()
        );
        let opencode = target("opencode");
        let result = opencode.install(Location::Global, install_options(true));
        assert_eq!(Path::new(&result.files[0].path), fixture.xdg_config_file());
        assert!(fixture.xdg_config_file().exists());
        assert!(!fixture.legacy_dir().exists());
    });
}

#[test]
fn honors_xdg_config_home_for_global_path_like_opencode_does() {
    with_xdg_fixture(|fixture| {
        let custom = fixture.fixture.home().join("xdg-custom");
        set_env("XDG_CONFIG_HOME", &custom);
        let opencode = target("opencode");
        let result = opencode.install(Location::Global, install_options(true));
        assert_eq!(
            Path::new(&result.files[0].path),
            custom.join("opencode").join("opencode.jsonc")
        );
    });
}

#[test]
fn install_leaves_pre_535_codegraph_appdata_entry_preserving_siblings_and_comments() {
    with_xdg_fixture(|fixture| {
        let legacy_file = fixture.legacy_dir().join("opencode.jsonc");
        write_text(
                &legacy_file,
                &[
                    "{",
                    "  // my servers",
                    "  \"$schema\": \"https://opencode.ai/config.json\",",
                    "  \"mcp\": {",
                    "    \"codegraph\": { \"type\": \"local\", \"command\": [\"codegraph\", \"serve\", \"--mcp\"], \"enabled\": true },",
                    "    \"other\": { \"type\": \"local\", \"command\": [\"other\"], \"enabled\": true }",
                    "  }",
                    "}",
                    "",
                ]
                .join("\n"),
            );
        write_text(
            fixture.legacy_dir().join("AGENTS.md"),
            &(LEGACY_BLOCK.to_owned() + "\n"),
        );
        let opencode = target("opencode");
        let result = opencode.install(Location::Global, install_options(true));
        assert!(fixture.xdg_config_file().exists());
        let legacy_text = read_text(&legacy_file);
        assert!(legacy_text.contains("codegraph"));
        assert!(legacy_text.contains("\"other\""));
        assert!(legacy_text.contains("// my servers"));
        assert!(fixture.legacy_dir().join("AGENTS.md").exists());
        let removed = result
            .files
            .iter()
            .filter(|file| file.action == WriteAction::Removed)
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        assert!(!removed.iter().any(|path| fixture.in_legacy_dir(path)));
    });
}

#[test]
fn uninstall_leaves_legacy_codegraph_appdata_entry() {
    with_xdg_fixture(|fixture| {
        let legacy_file = fixture.legacy_dir().join("opencode.json");
        write_text(
            &legacy_file,
            "{\n  \"mcp\": {\n    \"codegraph\": { \"type\": \"local\", \"command\": [\"codegraph\", \"serve\", \"--mcp\"], \"enabled\": true }\n  }\n}\n",
        );
        let opencode = target("opencode");
        let result = opencode.uninstall(Location::Global);
        assert!(read_text(&legacy_file).contains("codegraph"));
        assert!(
            !result.files.iter().any(
                |file| file.action == WriteAction::Removed && fixture.in_legacy_dir(&file.path)
            )
        );
    });
}

#[test]
fn install_after_install_does_not_report_codegraph_legacy_changes() {
    with_xdg_fixture(|fixture| {
        let legacy_file = fixture.legacy_dir().join("opencode.json");
        write_text(
            &legacy_file,
            "{\n  \"mcp\": {\n    \"codegraph\": { \"type\": \"local\", \"command\": [\"codegraph\", \"serve\", \"--mcp\"], \"enabled\": true }\n  }\n}\n",
        );
        let opencode = target("opencode");
        let first = opencode.install(Location::Global, install_options(true));
        assert!(
            !first.files.iter().any(
                |file| file.action == WriteAction::Removed && fixture.in_legacy_dir(&file.path)
            )
        );
        let second = opencode.install(Location::Global, install_options(true));
        assert!(
            !second
                .files
                .iter()
                .any(|file| fixture.in_legacy_dir(&file.path))
        );
        assert_eq!(
            second
                .files
                .iter()
                .find(|file| file.path.ends_with("opencode.jsonc"))
                .unwrap()
                .action,
            WriteAction::Unchanged
        );
    });
}

#[test]
fn detects_opencode_as_installed_from_legacy_only_appdata_dir_so_install_can_heal_it() {
    with_xdg_fixture(|fixture| {
        fs::create_dir_all(fixture.legacy_dir()).expect("failed to create legacy dir");
        let opencode = target("opencode");
        assert!(opencode.detect(Location::Global).installed);
        assert!(!opencode.detect(Location::Global).already_configured);
    });
}
