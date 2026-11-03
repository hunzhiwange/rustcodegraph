use super::common::*;
#[test]
fn case_041_bob_roundtrip() {
    with_fixture("041", |f| {
        let t = get_target("bob").expect("Bob registered");
        assert_eq!(
            resolve_target_flag("bob", Location::Local).unwrap().len(),
            1
        );
        for loc in [Location::Global, Location::Local] {
            assert!(t.supports_location(loc));
            let p = PathBuf::from(&t.describe_paths(loc)[0]);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(
                &p,
                r#"{"other":true,"mcpServers":{"custom":{"command":"keep"}}}"#,
            )
            .unwrap();
            t.install(loc, install_options(false));
            let text = fs::read_to_string(&p).unwrap();
            let v: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(v["mcpServers"]["custom"]["command"], "keep");
            assert_eq!(v["mcpServers"]["rustcodegraph"]["command"], "rustcodegraph");
            assert!(v["mcpServers"]["rustcodegraph"].get("type").is_none());
            assert_eq!(
                t.install(loc, install_options(true)).files[0].action,
                WriteAction::Unchanged
            );
            assert_eq!(text, fs::read_to_string(&p).unwrap());
            assert!(t.detect(loc).already_configured);
            assert!(t.print_config(loc).contains("rustcodegraph"));
            t.uninstall(loc);
            let v: Value = serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap();
            assert_eq!(v["other"], true);
            assert_eq!(v["mcpServers"]["custom"]["command"], "keep");
        }
        assert!(!f.home().join(".bob/AGENTS.md").exists());
    });
}
#[test]
fn case_041_missing_and_legacy_paths() {
    with_fixture("041-path", |f| {
        let t = get_target("bob").expect("Bob registered");
        assert!(!t.detect(Location::Global).installed);
        let dir = f.home().join(".bob/settings");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("mcp_settings.json"), "{}").unwrap();
        assert!(t.describe_paths(Location::Global)[0].ends_with("mcp_settings.json"));
        fs::write(dir.join("mcp.json"), "{}").unwrap();
        assert!(t.describe_paths(Location::Global)[0].ends_with("settings/mcp.json"));
    });
}
