use super::common::*;
#[test]
fn case_051_install() {
    with_fixture("051", |_| {
        let t = get_target("grok").expect("Grok registered");
        assert_eq!(
            resolve_target_flag("grok", Location::Local).unwrap().len(),
            1
        );
        for loc in [Location::Local, Location::Global] {
            assert!(t.supports_location(loc));
            let p = PathBuf::from(&t.describe_paths(loc)[0]);
            assert!(!t.detect(loc).already_configured);
            t.install(loc, install_options(false));
            assert!(t.detect(loc).already_configured);
            assert!(t.print_config(loc).contains("[mcp_servers.rustcodegraph]"));
            assert_eq!(
                t.install(loc, install_options(true)).files[0].action,
                WriteAction::Unchanged
            );
            assert!(!p.parent().unwrap().join("rules").exists());
        }
    });
}
#[test]
fn case_051_existing_preserved() {
    with_fixture("051-existing", |_| {
        let t = get_target("grok").unwrap();
        let p = PathBuf::from(&t.describe_paths(Location::Global)[0]);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        let original = "# user\n[mcp_servers.rustcodegraph]\ncommand = \"custom\"\nenabled = false\n\n[mcp_servers.other]\ncommand = \"other\"\n";
        fs::write(&p, original).unwrap();
        assert_eq!(
            t.install(Location::Global, install_options(false)).files[0].action,
            WriteAction::Unchanged
        );
        assert_eq!(fs::read_to_string(p).unwrap(), original);
    });
}
#[test]
fn case_051_override_and_sibling() {
    with_fixture("051-override", |f| {
        set_env("GROK_HOME", f.home().join("custom"));
        let t = get_target("grok").unwrap();
        let p = f.home().join("custom/config.toml");
        assert_eq!(PathBuf::from(&t.describe_paths(Location::Global)[0]), p);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "# keep\n[settings]\ntheme = \"dark\"\n").unwrap();
        t.install(Location::Global, install_options(false));
        assert!(
            fs::read_to_string(p)
                .unwrap()
                .starts_with("# keep\n[settings]\ntheme = \"dark\"\n")
        );
        assert_eq!(
            PathBuf::from(&t.describe_paths(Location::Local)[0]),
            f.cwd().canonicalize().unwrap().join(".grok/config.toml")
        );
    });
}

#[test]
fn case_051_conflicting_toml_preserved() {
    with_fixture("051-conflict", |_| {
        let t = get_target("grok").unwrap();
        let p = PathBuf::from(&t.describe_paths(Location::Global)[0]);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        for original in [
            "mcp_servers = 1\n",
            "mcp_servers = { other = { command = \"custom\" } }\n",
            "broken = [\n",
        ] {
            fs::write(&p, original).unwrap();
            t.install(Location::Global, install_options(false));
            assert_eq!(fs::read_to_string(&p).unwrap(), original);
        }
    });
}
