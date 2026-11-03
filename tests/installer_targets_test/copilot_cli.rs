use super::common::*;
#[test]
fn case_037_global_install() {
    with_fixture("037", |f| {
        let t = get_target("copilot-cli").expect("registered CLI target");
        let path = f.home().join(".copilot/mcp-config.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"other":true,"mcpServers":{"custom":{"command":"keep"}}}"#,
        )
        .unwrap();
        assert_eq!(
            t.install(Location::Global, install_options(false)).files[0].action,
            WriteAction::Updated
        );
        let text = fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["mcpServers"]["custom"]["command"], "keep");
        assert_eq!(v["mcpServers"]["rustcodegraph"]["tools"], json!(["*"]));
        assert_eq!(v["mcpServers"]["rustcodegraph"]["deferTools"], "never");
        assert!(t.detect(Location::Global).already_configured);
        assert_eq!(
            t.install(Location::Global, install_options(true)).files[0].action,
            WriteAction::Unchanged
        );
        assert_eq!(text, fs::read_to_string(path).unwrap());
        assert!(t.print_config(Location::Global).contains("rustcodegraph"));
        assert_eq!(
            resolve_target_flag("copilot-cli", Location::Global)
                .unwrap()
                .len(),
            1
        );
    });
}
#[test]
fn case_037_local_rejected() {
    with_fixture("037-local", |f| {
        let t = get_target("copilot-cli").unwrap();
        assert!(!t.supports_location(Location::Local));
        assert!(!t.detect(Location::Local).installed);
        assert!(
            t.install(Location::Local, install_options(false))
                .files
                .is_empty()
        );
        assert!(t.print_config(Location::Local).contains("global"));
        assert!(t.describe_paths(Location::Local).is_empty());
        assert!(!f.home().join(".copilot").exists());
    });
}
#[test]
fn case_037_override_and_ide_only() {
    with_fixture("037-override", |f| {
        let saved_path = env::var_os("PATH");
        set_env("PATH", "");
        let dir = f.home().join("override");
        set_env("COPILOT_HOME", &dir);
        fs::create_dir_all(dir.join("ide")).unwrap();
        let t = get_target("copilot-cli").unwrap();
        let detected = t.detect(Location::Global).installed;
        match saved_path {
            Some(p) => set_env_os("PATH", p),
            None => remove_env("PATH"),
        };
        assert!(!detected);
        assert_eq!(
            t.install(Location::Global, install_options(false)).files[0].action,
            WriteAction::Created
        );
        assert!(dir.join("mcp-config.json").exists());
        assert!(t.detect(Location::Global).installed);
        assert!(!f.cwd().join("AGENTS.md").exists());
    });
}

#[test]
fn case_038_roundtrip_preserves_user_configuration() {
    with_fixture("038-roundtrip", |f| {
        let t = get_target("copilot-cli").unwrap();
        let path = f.home().join(".copilot/mcp-config.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{\r\n  // user comment\r\n  \"inputs\": [],\r\n  \"mcpServers\": {\"other\": {\"command\":\"custom\"}, \"codegraph\": {} , \"rustcodegraph\": {\"disabled\":true,\"env\":{\"CUSTOM\":\"yes\"},\"cwd\":\"custom-dir\"}}\r\n}\r\n").unwrap();
        t.install(Location::Global, install_options(false));
        let installed = fs::read_to_string(&path).unwrap();
        assert!(installed.contains("// user comment"));
        assert!(!installed.replace("\r\n", "").contains('\n'));
        let v = jsonc_parser::parse_to_serde_value(&installed, &Default::default())
            .unwrap()
            .unwrap();
        assert_eq!(v["mcpServers"]["rustcodegraph"]["disabled"], true);
        assert_eq!(v["mcpServers"]["rustcodegraph"]["cwd"], "custom-dir");
        assert_eq!(v["mcpServers"]["rustcodegraph"]["env"]["CUSTOM"], "yes");
        assert_eq!(
            t.install(Location::Global, install_options(false)).files[0].action,
            WriteAction::Unchanged
        );
        assert_eq!(installed, fs::read_to_string(&path).unwrap());
        assert_eq!(
            t.uninstall(Location::Global).files[0].action,
            WriteAction::Removed
        );
        let removed = fs::read_to_string(&path).unwrap();
        assert!(removed.contains("// user comment"));
        let v = jsonc_parser::parse_to_serde_value(&removed, &Default::default())
            .unwrap()
            .unwrap();
        assert!(v["mcpServers"].get("rustcodegraph").is_none());
        assert_eq!(v["mcpServers"]["other"]["command"], "custom");
        assert!(v["mcpServers"].get("codegraph").is_some());
        assert_eq!(
            t.uninstall(Location::Global).files[0].action,
            WriteAction::NotFound
        );
        assert_eq!(removed, fs::read_to_string(path).unwrap());
    });
}

#[test]
fn case_038_empty_wrapper_removed_and_file_retained() {
    with_fixture("038-empty", |f| {
        let t = get_target("copilot-cli").unwrap();
        t.install(Location::Global, install_options(false));
        t.uninstall(Location::Global);
        let path = f.home().join(".copilot/mcp-config.json");
        let v: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert!(v.get("mcpServers").is_none());
    });
}

#[test]
fn case_038_missing_and_empty_are_unchanged() {
    with_fixture("038-missing", |f| {
        let t = get_target("copilot-cli").unwrap();
        assert_eq!(
            t.uninstall(Location::Global).files[0].action,
            WriteAction::NotFound
        );
        let path = f.home().join(".copilot/mcp-config.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for text in ["", "  \r\n", "{}\r\n", "{\"inputs\":[]}"] {
            fs::write(&path, text).unwrap();
            assert_eq!(
                t.uninstall(Location::Global).files[0].action,
                WriteAction::NotFound
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
    });
}
