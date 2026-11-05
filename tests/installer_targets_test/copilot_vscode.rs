use super::common::*;

#[test]
fn case_035_local_install_and_idempotency() {
    with_fixture("035-local", |f| {
        let t = get_target("copilot-vscode").expect("VS Code target registered");
        assert!(!t.detect(Location::Local).installed);
        let result = t.install(Location::Local, install_options(false));
        assert_eq!(result.files[0].action, WriteAction::Created);
        let path = f.cwd().join(".vscode/mcp.json");
        let text = fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            v["servers"]["rustcodegraph"]["args"][3],
            env::current_dir().unwrap().to_string_lossy().as_ref()
        );
        assert!(t.detect(Location::Local).already_configured);
        assert_eq!(
            t.install(Location::Local, install_options(true)).files[0].action,
            WriteAction::Unchanged
        );
        assert_eq!(text, fs::read_to_string(path).unwrap());
        assert!(!f.cwd().join("AGENTS.md").exists());
        assert_eq!(
            resolve_target_flag("copilot-vscode", Location::Local)
                .unwrap()
                .len(),
            1
        );
    });
}

#[test]
fn case_035_global_has_no_workspace_variable() {
    with_fixture("035-global", |_| {
        let t = get_target("copilot-vscode").expect("VS Code target registered");
        let config = t.print_config(Location::Global);
        assert!(!config.contains("--path"));
        assert!(!config.contains("workspaceFolder"));
        assert!(config.contains("servers"));
        assert!(t.supports_location(Location::Global));
        assert!(t.supports_location(Location::Local));
        t.install(Location::Global, install_options(false));
        assert!(t.detect(Location::Global).already_configured);
    });
}

#[test]
fn case_035_jsonc_preserves_siblings_and_rejects_invalid() {
    with_fixture("035-jsonc", |f| {
        let t = get_target("copilot-vscode").expect("VS Code target registered");
        let path = f.cwd().join(".vscode/mcp.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "{ // keep\n\"inputs\": [], \"servers\": {\"other\": {\"command\":\"custom\"},},}\n",
        )
        .unwrap();
        t.install(Location::Local, install_options(false));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("// keep"));
        assert!(text.contains("\"custom\""));
        assert!(text.contains("\"inputs\""));
        fs::write(&path, "{ broken").unwrap();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || t.install(Location::Local, install_options(false))
            ))
            .is_err()
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "{ broken");
    });
}

#[test]
fn case_035_updates_managed_fields_preserving_custom_keys() {
    with_fixture("035-update", |f| {
        let t = get_target("copilot-vscode").unwrap();
        let path = f.cwd().join(".vscode/mcp.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"servers":{"rustcodegraph":{"command":"old","env":{"CUSTOM":"yes"}}}}"#,
        )
        .unwrap();
        t.install(Location::Local, install_options(false));
        let text = fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["servers"]["rustcodegraph"]["command"], "rustcodegraph");
        assert_eq!(v["servers"]["rustcodegraph"]["env"]["CUSTOM"], "yes");
        assert_eq!(
            t.install(Location::Local, install_options(false)).files[0].action,
            WriteAction::Unchanged
        );
        fs::write(&path, r#"{"servers":[]}"#).unwrap();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || t.install(Location::Local, install_options(false))
            ))
            .is_err()
        );
        assert_eq!(fs::read_to_string(path).unwrap(), r#"{"servers":[]}"#);
    });
}

#[test]
fn case_036_roundtrip_preserves_user_configuration() {
    with_fixture("036-roundtrip", |f| {
        let t = get_target("copilot-vscode").unwrap();
        let path = f.cwd().join(".vscode/mcp.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{\r\n  // user comment\r\n  \"inputs\": [],\r\n  \"servers\": {\"other\": {\"command\":\"custom\"}, \"codegraph\": {} , \"rustcodegraph\": {\"disabled\":true,\"env\":{\"CUSTOM\":\"yes\"},\"cwd\":\"custom-dir\"}}\r\n}\r\n").unwrap();
        t.install(Location::Local, install_options(false));
        let installed = fs::read_to_string(&path).unwrap();
        assert!(installed.contains("// user comment"));
        assert!(!installed.replace("\r\n", "").contains('\n'));
        let v = jsonc_parser::parse_to_serde_value(&installed, &Default::default())
            .unwrap()
            .unwrap();
        assert_eq!(v["servers"]["rustcodegraph"]["disabled"], true);
        assert_eq!(v["servers"]["rustcodegraph"]["cwd"], "custom-dir");
        assert_eq!(v["servers"]["rustcodegraph"]["env"]["CUSTOM"], "yes");
        assert_eq!(
            t.install(Location::Local, install_options(false)).files[0].action,
            WriteAction::Unchanged
        );
        assert_eq!(installed, fs::read_to_string(&path).unwrap());
        assert_eq!(
            t.uninstall(Location::Local).files[0].action,
            WriteAction::Removed
        );
        let removed = fs::read_to_string(&path).unwrap();
        assert!(removed.contains("// user comment"));
        let v = jsonc_parser::parse_to_serde_value(&removed, &Default::default())
            .unwrap()
            .unwrap();
        assert!(v["servers"].get("rustcodegraph").is_none());
        assert_eq!(v["servers"]["other"]["command"], "custom");
        assert!(v["servers"].get("codegraph").is_some());
        assert_eq!(
            t.uninstall(Location::Local).files[0].action,
            WriteAction::NotFound
        );
        assert_eq!(removed, fs::read_to_string(path).unwrap());
    });
}

#[test]
fn case_036_empty_wrapper_removed_and_file_retained() {
    with_fixture("036-empty", |f| {
        let t = get_target("copilot-vscode").unwrap();
        t.install(Location::Local, install_options(false));
        t.uninstall(Location::Local);
        let path = f.cwd().join(".vscode/mcp.json");
        let v: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert!(v.get("servers").is_none());
    });
}

#[test]
fn case_036_missing_and_empty_are_unchanged() {
    with_fixture("036-missing", |f| {
        let t = get_target("copilot-vscode").unwrap();
        assert_eq!(
            t.uninstall(Location::Local).files[0].action,
            WriteAction::NotFound
        );
        let path = f.cwd().join(".vscode/mcp.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for text in ["", "  \r\n", "{}\r\n", "{\"inputs\":[]}"] {
            fs::write(&path, text).unwrap();
            assert_eq!(
                t.uninstall(Location::Local).files[0].action,
                WriteAction::NotFound
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
    });
}
