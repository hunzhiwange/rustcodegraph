use super::common::*;
#[test]
fn case_043_droid_roundtrip() {
    with_fixture("043", |f| {
        let t = get_target("droid").expect("Droid registered");
        assert_eq!(
            resolve_target_flag("droid", Location::Local).unwrap().len(),
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
            assert!(v["mcpServers"]["rustcodegraph"].get("type").is_some());
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
        assert!(!f.home().join(".droid/AGENTS.md").exists());
    });
}

#[test]
fn case_043_absent_configuration() {
    with_fixture("043-absent", |_| {
        let t = get_target("droid").expect("Droid registered");
        for loc in [Location::Local, Location::Global] {
            assert!(!t.detect(loc).installed);
            t.uninstall(loc);
            assert!(!PathBuf::from(&t.describe_paths(loc)[0]).exists());
        }
    });
}
#[test]
fn case_043_preserves_custom_entry() {
    with_fixture("043-custom", |_| {
        let t = get_target("droid").expect("Droid registered");
        let p = PathBuf::from(&t.describe_paths(Location::Local)[0]);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        let before = r#"{"mcpServers":{"rustcodegraph":{"command":"custom","type":"stdio"}}}"#;
        fs::write(&p, before).unwrap();
        t.install(Location::Local, install_options(false));
        let after = fs::read_to_string(&p).unwrap();
        assert!(after.contains("custom"));
        assert_eq!(
            t.install(Location::Local, install_options(false)).files[0].action,
            WriteAction::Unchanged
        );
        assert_eq!(fs::read_to_string(p).unwrap(), after);
    });
}
