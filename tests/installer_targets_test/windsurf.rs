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

#[test]
fn case_046_custom_crlf_roundtrip() {
    with_fixture("046-custom", |_| {
        let t = get_target("windsurf").unwrap();
        let loc = Location::Global;
        let p = PathBuf::from(&t.describe_paths(loc)[0]);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        let before = "{\r\n  // user settings\r\n  \"enabled\": false,\r\n  \"mcpServers\": {\r\n    \"custom\": {\"command\": \"keep\"},\r\n    \"rustcodegraph\": {\"command\": \"custom-launch\", \"args\": [\"custom\"], \"type\": \"stdio\", \"disabled\": true}\r\n  }\r\n}\r\n";
        fs::write(&p, before).unwrap();
        for _ in 0..2 {
            assert_eq!(
                t.install(loc, install_options(false)).files[0].action,
                WriteAction::Unchanged
            );
            assert_eq!(fs::read_to_string(&p).unwrap(), before);
        }
        assert_eq!(t.uninstall(loc).files[0].action, WriteAction::Removed);
        let after = fs::read_to_string(&p).unwrap();
        assert!(after.contains("// user settings"));
        assert!(!after.replace("\r\n", "").contains('\n'));
        let v = jsonc_parser::parse_to_serde_value(&after, &Default::default())
            .unwrap()
            .unwrap();
        assert_eq!(v["enabled"], false);
        assert_eq!(v["mcpServers"]["custom"]["command"], "keep");
        assert!(v["mcpServers"].get("rustcodegraph").is_none());
        t.uninstall(loc);
        assert_eq!(fs::read_to_string(p).unwrap(), after);
    });
}

#[test]
fn case_046_empty_and_absent_roundtrip() {
    with_fixture("046-empty", |_| {
        let t = get_target("windsurf").unwrap();
        let loc = Location::Global;
        let p = PathBuf::from(&t.describe_paths(loc)[0]);
        t.uninstall(loc);
        assert!(!p.exists());
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        for before in ["", "{}", "{\n // keep\n \"enabled\": false\n}\n"] {
            fs::write(&p, before).unwrap();
            t.uninstall(loc);
            assert_eq!(fs::read_to_string(&p).unwrap(), before);
            t.install(loc, install_options(false));
            let installed = fs::read_to_string(&p).unwrap();
            assert_eq!(
                t.install(loc, install_options(false)).files[0].action,
                WriteAction::Unchanged
            );
            assert_eq!(fs::read_to_string(&p).unwrap(), installed);
            t.uninstall(loc);
            let removed = fs::read_to_string(&p).unwrap();
            t.uninstall(loc);
            assert_eq!(fs::read_to_string(&p).unwrap(), removed);
            assert!(!removed.contains("rustcodegraph"));
        }
    });
}

#[test]
fn case_046_unmanaged_configuration_unchanged() {
    with_fixture("046-unmanaged", |_| {
        let t = get_target("windsurf").unwrap();
        let p = PathBuf::from(&t.describe_paths(Location::Global)[0]);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        let before =
            "{\r\n // keep\r\n \"mcpServers\": {\"other\": {\"command\": \"keep\"}}\r\n}\r\n";
        fs::write(&p, before).unwrap();
        for _ in 0..2 {
            t.uninstall(Location::Global);
            assert_eq!(fs::read_to_string(&p).unwrap(), before);
        }
    });
}
