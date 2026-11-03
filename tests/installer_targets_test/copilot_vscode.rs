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
            f.cwd().canonicalize().unwrap().to_string_lossy().as_ref()
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
