use super::common::*;
#[test]
fn case_053_install() {
    with_fixture("053", |f| {
        let t = get_target("kimi").expect("Kimi registered");
        assert_eq!(
            resolve_target_flag("kimi", Location::Local).unwrap().len(),
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
fn case_053_override() {
    with_fixture("053-override", |f| {
        set_env("KIMI_CODE_HOME", f.home().join("override"));
        let t = get_target("kimi").expect("Kimi registered");
        assert_eq!(
            PathBuf::from(&t.describe_paths(Location::Global)[0]),
            f.home().join("override/mcp.json")
        );
        assert_eq!(
            PathBuf::from(&t.describe_paths(Location::Local)[0]),
            f.cwd().canonicalize().unwrap().join(".kimi-code/mcp.json")
        );
    });
}
#[test]
fn case_053_absent() {
    with_fixture("053-absent", |_| {
        let t = get_target("kimi").expect("Kimi registered");
        assert!(!t.detect(Location::Local).already_configured);
        assert!(!PathBuf::from(&t.describe_paths(Location::Local)[0]).exists());
    });
}
#[test]
fn case_053_fresh_schema_and_notes() {
    with_fixture("053-fresh", |f| {
        let t = get_target("kimi").unwrap();
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
            assert!(notes.contains("new Kimi"));
            assert_eq!(notes.contains("trusted"), loc == Location::Local);
            assert_eq!(
                t.install(loc, install_options(false)).files[0].action,
                WriteAction::Unchanged
            );
        }
        assert!(!f.cwd().join("AGENTS.md").exists());
        assert!(!f.home().join(".kimi-code/AGENTS.md").exists());
    });
}
