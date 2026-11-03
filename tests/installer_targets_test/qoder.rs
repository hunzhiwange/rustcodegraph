use super::common::*;
#[test]
fn case_055_install() {
    with_fixture("055", |f| {
        let t = get_target("qoder").expect("Qoder registered");
        assert_eq!(
            resolve_target_flag("qoder", Location::Local).unwrap().len(),
            1
        );
        for loc in [Location::Local, Location::Global] {
            assert!(t.supports_location(loc));
            let p = PathBuf::from(&t.describe_paths(loc)[0]);
            assert!(!t.detect(loc).installed);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(
                &p,
                r#"{ // keep
 "custom":true,"mcpServers":{"other":{"command":"keep"}}}"#,
            )
            .unwrap();
            t.install(loc, install_options(false));
            let text = fs::read_to_string(&p).unwrap();
            assert!(text.contains("// keep"));
            assert!(text.contains("other"));
            assert!(t.detect(loc).already_configured);
            assert_eq!(
                t.install(loc, install_options(false)).files[0].action,
                WriteAction::Unchanged
            );
            assert_eq!(text, fs::read_to_string(p).unwrap());
            assert!(t.print_config(loc).contains("rustcodegraph"));
        }
        assert!(!f.cwd().join("AGENTS.md").exists());
    });
}
#[test]
fn case_055_override() {
    with_fixture("055-override", |f| {
        set_env("QODER_CONFIG_DIR", f.home().join("override"));
        let t = get_target("qoder").expect("Qoder registered");
        assert_eq!(
            PathBuf::from(&t.describe_paths(Location::Global)[0]),
            f.home().join("override/settings.json")
        );
        assert_eq!(
            PathBuf::from(&t.describe_paths(Location::Local)[0]),
            f.cwd()
                .canonicalize()
                .unwrap()
                .join(".qoder/settings.local.json")
        );
    });
}
#[test]
fn case_055_absent() {
    with_fixture("055-absent", |_| {
        let t = get_target("qoder").expect("Qoder registered");
        assert!(!t.detect(Location::Local).already_configured);
        assert!(!PathBuf::from(&t.describe_paths(Location::Local)[0]).exists());
    });
}
#[test]
fn case_055_fresh_schema_and_notes() {
    with_fixture("055-fresh", |f| {
        let t = get_target("qoder").unwrap();
        for loc in [Location::Local, Location::Global] {
            let result = t.install(loc, install_options(true));
            assert_eq!(result.files[0].action, WriteAction::Created);
            let p = PathBuf::from(&t.describe_paths(loc)[0]);
            let value: Value = serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap();
            let server = &value["mcpServers"]["rustcodegraph"];
            assert!(server["command"].is_string());
            assert!(server["args"].is_array());
            assert!(server.get("type").is_none());
            let notes = result.notes.unwrap().join(" ");
            assert!(notes.contains("Qoder CLI"));
            assert_eq!(notes.contains("trusted"), loc == Location::Local);
            assert_eq!(
                t.install(loc, install_options(false)).files[0].action,
                WriteAction::Unchanged
            );
        }
        assert!(!f.cwd().join("AGENTS.md").exists());
        assert!(!f.home().join(".qoder/AGENTS.md").exists());
    });
}

#[test]
fn case_056_roundtrip() {
    with_fixture("056-roundtrip", |_| {
        let t = get_target("qoder").unwrap();
        for loc in [Location::Local, Location::Global] {
            let p = PathBuf::from(&t.describe_paths(loc)[0]);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(
                &p,
                "{\r\n // keep\r\n \"mcpServers\":{\"other\":{\"command\":\"keep\"}}\r\n}\r\n",
            )
            .unwrap();
            t.install(loc, install_options(false));
            let installed = fs::read(&p).unwrap();
            assert_eq!(
                t.install(loc, install_options(true)).files[0].action,
                WriteAction::Unchanged
            );
            assert_eq!(fs::read(&p).unwrap(), installed);
            assert_eq!(t.uninstall(loc).files[0].action, WriteAction::Removed);
            let removed = fs::read_to_string(&p).unwrap();
            assert!(removed.contains("// keep\r\n"));
            assert!(removed.contains("\"other\":{\"command\":\"keep\"}"));
            assert!(!removed.contains("rustcodegraph"));
            assert_eq!(t.uninstall(loc).files[0].action, WriteAction::NotFound);
            assert_eq!(fs::read_to_string(&p).unwrap(), removed);
        }
    });
}

#[test]
fn case_056_custom_reinstall() {
    with_fixture("056-custom", |_| {
        let t = get_target("qoder").unwrap();
        let loc = Location::Global;
        let p = PathBuf::from(&t.describe_paths(loc)[0]);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        let original = r#"{ // user
"mcpServers":{"rustcodegraph":{"command":"custom","args":["custom"],"enabled":false,"env":{"CUSTOM":"yes"}},"codegraph":{"command":"legacy"}}}"#;
        fs::write(&p, original).unwrap();
        for _ in 0..2 {
            assert_eq!(
                t.install(loc, install_options(true)).files[0].action,
                WriteAction::Unchanged
            );
            assert_eq!(fs::read_to_string(&p).unwrap(), original);
        }
        t.uninstall(loc);
        let remaining = fs::read_to_string(p).unwrap();
        assert!(remaining.contains("\"codegraph\":{\"command\":\"legacy\"}"));
        assert!(!remaining.contains("CUSTOM"));
    });
}

#[test]
fn case_056_empty_missing() {
    with_fixture("056-empty", |_| {
        let t = get_target("qoder").unwrap();
        let loc = Location::Global;
        assert_eq!(t.uninstall(loc).files[0].action, WriteAction::NotFound);
        let p = PathBuf::from(&t.describe_paths(loc)[0]);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        for original in ["", "{}", "{ // keep\n}"] {
            fs::write(&p, original).unwrap();
            assert_eq!(t.uninstall(loc).files[0].action, WriteAction::NotFound);
            assert_eq!(fs::read_to_string(&p).unwrap(), original);
            t.install(loc, install_options(false));
            t.uninstall(loc);
            assert!(!fs::read_to_string(&p).unwrap().contains("mcpServers"));
            assert_eq!(t.uninstall(loc).files[0].action, WriteAction::NotFound);
        }
    });
}
