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
