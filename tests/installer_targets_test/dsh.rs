use super::common::*;
#[test]
fn case_049_install() {
    with_fixture("049", |f| {
        let t = get_target("dsh").expect("DSH registered");
        assert_eq!(
            resolve_target_flag("dsh", Location::Global).unwrap().len(),
            1
        );
        let p = PathBuf::from(&t.describe_paths(Location::Global)[0]);
        assert!(!t.detect(Location::Global).installed);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "# keep\n- remove: unrelated\n").unwrap();
        t.install(Location::Global, install_options(false));
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.starts_with("# keep\n- remove: unrelated\n"));
        assert!(text.contains("serverName: rustcodegraph"));
        assert!(text.contains("@deepseek-ai/dsh-mcp-client"));
        assert!(t.detect(Location::Global).already_configured);
        assert_eq!(
            t.install(Location::Global, install_options(true)).files[0].action,
            WriteAction::Unchanged
        );
        assert_eq!(text, fs::read_to_string(&p).unwrap());
        assert!(
            t.print_config(Location::Global)
                .contains("transport: stdio")
        );
        assert!(!f.home().join(".dsh/AGENTS.md").exists());
    });
}
#[test]
fn case_049_local_refused() {
    with_fixture("049-local", |_| {
        let t = get_target("dsh").expect("DSH registered");
        assert!(!t.supports_location(Location::Local));
        assert!(
            t.install(Location::Local, install_options(false))
                .files
                .is_empty()
        );
        assert!(t.describe_paths(Location::Local).is_empty());
    });
}
#[test]
fn case_049_invalid_preserved() {
    with_fixture("049-invalid", |_| {
        let t = get_target("dsh").expect("DSH registered");
        let p = PathBuf::from(&t.describe_paths(Location::Global)[0]);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "mapping: value\n").unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            t.install(Location::Global, install_options(false))
        }));
        assert_eq!(fs::read_to_string(p).unwrap(), "mapping: value\n");
        assert!(!t.detect(Location::Global).already_configured);
    });
}
#[test]
fn case_049_override_empty_and_profile() {
    with_fixture("049-profile", |f| {
        set_env("DSH_HOME", f.home().join("override"));
        let t = get_target("dsh").unwrap();
        let p = f.home().join("override/cordis.patch.yml");
        assert_eq!(PathBuf::from(&t.describe_paths(Location::Global)[0]), p);
        t.install(Location::Global, install_options(false));
        assert!(fs::read_to_string(&p).unwrap().starts_with("- insert:"));
        fs::write(&p, "# keep\n[]\n").unwrap();
        t.install(Location::Global, install_options(false));
        assert!(t.detect(Location::Global).already_configured);
        fs::write(&p, "[]\n").unwrap();
        let profile = f.home().join("override/profiles/default/cordis.patch.yml");
        fs::create_dir_all(profile.parent().unwrap()).unwrap();
        fs::write(&profile, "- insert:\n    - name: '@deepseek-ai/dsh-mcp-client'\n      config: {serverName: rustcodegraph, command: custom}\n").unwrap();
        assert!(t.detect(Location::Global).already_configured);
        assert_eq!(
            t.install(Location::Global, install_options(false)).files[0].action,
            WriteAction::Unchanged
        );
        assert_eq!(fs::read_to_string(p).unwrap(), "[]\n");
    });
}

#[test]
fn case_050_crlf_roundtrip() {
    with_fixture("050-crlf", |_| {
        let t = get_target("dsh").unwrap();
        let p = PathBuf::from(&t.describe_paths(Location::Global)[0]);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        let original = "# keep\r\n- remove: unrelated\r\n";
        fs::write(&p, original).unwrap();
        t.install(Location::Global, install_options(false));
        let installed = fs::read(&p).unwrap();
        assert_eq!(
            t.install(Location::Global, install_options(true)).files[0].action,
            WriteAction::Unchanged
        );
        assert_eq!(fs::read(&p).unwrap(), installed);
        assert_eq!(
            t.uninstall(Location::Global).files[0].action,
            WriteAction::Removed
        );
        assert_eq!(fs::read_to_string(&p).unwrap(), original);
        let bytes = fs::read(&p).unwrap();
        t.uninstall(Location::Global);
        assert_eq!(fs::read(&p).unwrap(), bytes);
    });
}

#[test]
fn case_050_custom_profile_and_sibling() {
    with_fixture("050-custom", |f| {
        let t = get_target("dsh").unwrap();
        let p = f.home().join(".dsh/profiles/default/cordis.patch.yml");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        let sibling = "    - name: other\n      config: {serverName: other, enabled: false}\n";
        let text = format!(
            "# keep\n- insert:\n    - name: '@deepseek-ai/dsh-mcp-client'\n      disabled: true\n      config: {{serverName: rustcodegraph, command: custom}}\n{sibling}"
        );
        fs::write(&p, &text).unwrap();
        t.install(Location::Global, install_options(true));
        assert_eq!(fs::read_to_string(&p).unwrap(), text);
        t.uninstall(Location::Global);
        assert_eq!(
            fs::read_to_string(&p).unwrap(),
            format!("# keep\n- insert:\n{sibling}")
        );
        let after = fs::read(&p).unwrap();
        t.uninstall(Location::Global);
        assert_eq!(fs::read(&p).unwrap(), after);
    });
}

#[test]
fn case_050_empty_and_uneditable() {
    with_fixture("050-empty", |_| {
        let t = get_target("dsh").unwrap();
        let p = PathBuf::from(&t.describe_paths(Location::Global)[0]);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "# keep\n[]\n").unwrap();
        t.install(Location::Global, install_options(false));
        t.uninstall(Location::Global);
        assert_eq!(fs::read_to_string(&p).unwrap(), "# keep\n[]\n");
        let text = "- insert:\n    - name: group\n      config:\n        name: '@deepseek-ai/dsh-mcp-client'\n        serverName: rustcodegraph\n";
        fs::write(&p, text).unwrap();
        t.uninstall(Location::Global);
        assert_eq!(fs::read_to_string(&p).unwrap(), text);
    });
}
