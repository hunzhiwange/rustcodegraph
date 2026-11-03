use super::common::*;
#[test]
fn case_047_install() {
    with_fixture("047", |f| {
        let t = get_target("qwen").expect("Qwen registered");
        assert_eq!(
            resolve_target_flag("qwen", Location::Local).unwrap().len(),
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
        assert!(!f.cwd().join("QWEN.md").exists());
    });
}
#[test]
fn case_047_override() {
    with_fixture("047-override", |f| {
        set_env("QWEN_HOME", f.home().join("override"));
        let t = get_target("qwen").expect("Qwen registered");
        assert_eq!(
            PathBuf::from(&t.describe_paths(Location::Global)[0]),
            f.home().join("override/settings.json")
        );
        assert_eq!(
            PathBuf::from(&t.describe_paths(Location::Local)[0]),
            f.cwd().canonicalize().unwrap().join(".qwen/settings.json")
        );
    });
}
#[test]
fn case_047_absent() {
    with_fixture("047-absent", |_| {
        let t = get_target("qwen").expect("Qwen registered");
        assert!(!t.detect(Location::Local).already_configured);
        assert!(!PathBuf::from(&t.describe_paths(Location::Local)[0]).exists());
    });
}

#[test]
fn case_047_auto_allow() {
    with_fixture("047-allow", |_| {
        let t = get_target("qwen").unwrap();
        t.install(Location::Local, install_options(true));
        let p = PathBuf::from(&t.describe_paths(Location::Local)[0]);
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.contains("mcp__rustcodegraph"));
        assert_eq!(
            t.install(Location::Local, install_options(true)).files[0].action,
            WriteAction::Unchanged
        );
        assert_eq!(text, fs::read_to_string(p).unwrap());
    });
}
