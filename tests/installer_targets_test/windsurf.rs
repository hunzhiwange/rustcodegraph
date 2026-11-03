use super::common::*;
#[test]
fn case_045_global_install() {
    with_fixture("045-global", |f| {
        let t = get_target("windsurf").expect("Windsurf registered");
        assert_eq!(
            resolve_target_flag("windsurf", Location::Global)
                .unwrap()
                .len(),
            1
        );
        let p = f.home().join(".codeium/windsurf/mcp_config.json");
        assert!(!t.detect(Location::Global).installed);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(
            &p,
            r#"{"custom":true,"mcpServers":{"other":{"command":"keep"}}}"#,
        )
        .unwrap();
        t.install(Location::Global, install_options(false));
        let text = fs::read_to_string(&p).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["mcpServers"]["rustcodegraph"]["command"], "rustcodegraph");
        assert!(v["mcpServers"]["rustcodegraph"].get("type").is_none());
        assert_eq!(v["mcpServers"]["other"]["command"], "keep");
        assert_eq!(
            t.install(Location::Global, install_options(true)).files[0].action,
            WriteAction::Unchanged
        );
        assert_eq!(text, fs::read_to_string(&p).unwrap());
        assert!(t.detect(Location::Global).already_configured);
        assert!(t.print_config(Location::Global).contains("mcpServers"));
    });
}
#[test]
fn case_045_reject_local() {
    with_fixture("045-local", |_| {
        let t = get_target("windsurf").expect("Windsurf registered");
        assert!(!t.supports_location(Location::Local));
        assert!(
            t.install(Location::Local, install_options(false))
                .files
                .is_empty()
        );
        assert!(t.describe_paths(Location::Local).is_empty());
        assert!(!t.detect(Location::Local).installed);
        assert!(
            t.print_config(Location::Local)
                .contains("--location=global")
        );
    });
}
#[test]
fn case_045_absent_uninstall() {
    with_fixture("045-absent", |f| {
        let t = get_target("windsurf").expect("Windsurf registered");
        t.uninstall(Location::Global);
        assert!(!f.home().join(".codeium").exists());
    });
}
