use super::common::*;
#[test]
fn case_057_install() {
    with_fixture("057", |f| {
        let t = get_target("pi").expect("Pi registered");
        assert_eq!(resolve_target_flag("pi", Location::Local).unwrap().len(), 1);
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
fn case_057_override() {
    with_fixture("057-override", |f| {
        set_env("PI_CODING_AGENT_DIR", f.home().join("override"));
        let t = get_target("pi").expect("Pi registered");
        assert_eq!(
            PathBuf::from(&t.describe_paths(Location::Global)[0]),
            f.home().join("override/mcp.json")
        );
        assert_eq!(
            PathBuf::from(&t.describe_paths(Location::Local)[0]),
            env::current_dir().unwrap().join(".pi/mcp.json")
        );
    });
}
#[test]
fn case_057_absent() {
    with_fixture("057-absent", |_| {
        let t = get_target("pi").expect("Pi registered");
        assert!(!t.detect(Location::Local).already_configured);
        assert!(!PathBuf::from(&t.describe_paths(Location::Local)[0]).exists());
    });
}
#[test]
fn case_057_fresh_schema_and_notes() {
    with_fixture("057-fresh", |f| {
        let t = get_target("pi").unwrap();
        for loc in [Location::Local, Location::Global] {
            let result = t.install(loc, install_options(true));
            assert_eq!(result.files[0].action, WriteAction::Created);
            let p = PathBuf::from(&t.describe_paths(loc)[0]);
            let value: Value = serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap();
            let server = &value["mcpServers"]["rustcodegraph"];
            assert!(server["command"].is_string());
            assert!(server["args"].is_array());
            assert!(server.get("type").is_none());
            assert_eq!(server["exposure"], "direct");
            let notes = result.notes.unwrap().join(" ");
            assert!(notes.contains("/reload"));
            assert_eq!(notes.contains("trusted"), loc == Location::Local);
            assert_eq!(
                t.install(loc, install_options(false)).files[0].action,
                WriteAction::Unchanged
            );
        }
        assert!(!f.cwd().join("AGENTS.md").exists());
        assert!(!f.home().join(".pi/AGENTS.md").exists());
    });
}

#[test]
fn case_058_roundtrips() {
    with_fixture("058", |_| {
        let t = get_target("pi").unwrap();
        for original in [
            "",
            "{}",
            "{\r\n // keep\r\n \"mcpServers\":{\"other\":{},\"rustcodegraph\":{\"command\":\"custom\",\"args\":[],\"exposure\":\"direct\",\"enabled\":false}}\r\n}",
        ] {
            let loc = Location::Local;
            let p = PathBuf::from(&t.describe_paths(loc)[0]);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, original).unwrap();
            t.install(loc, install_options(false));
            let installed = fs::read_to_string(&p).unwrap();
            if original.contains("custom") {
                assert_eq!(installed, original);
            }
            assert_eq!(
                t.install(loc, install_options(false)).files[0].action,
                WriteAction::Unchanged
            );
            assert_eq!(fs::read_to_string(&p).unwrap(), installed);
            assert_eq!(t.uninstall(loc).files[0].action, WriteAction::Removed);
            let after = fs::read_to_string(&p).unwrap();
            assert!(!after.contains("rustcodegraph"));
            if original.contains("other") {
                assert!(after.contains("other"));
                assert!(after.contains("// keep"));
                assert!(after.contains("\r\n"));
            }
            assert_eq!(t.uninstall(loc).files[0].action, WriteAction::NotFound);
            assert_eq!(fs::read_to_string(&p).unwrap(), after);
        }
    });
}
