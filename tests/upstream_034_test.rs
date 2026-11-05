#[path = "installer_targets_test/common.rs"]
#[allow(dead_code, unused_imports)]
mod common;

#[test]
fn case_034() {
    let _lock = common::TEST_LOCK.lock().unwrap();
    let keys = [
        "COPILOT_HOME",
        "QWEN_HOME",
        "DSH_HOME",
        "GROK_HOME",
        "KIMI_CODE_HOME",
        "QODER_CONFIG_DIR",
        "PI_CODING_AGENT_DIR",
    ];
    let saved: Vec<_> = keys
        .iter()
        .map(|key| (*key, std::env::var_os(key)))
        .collect();
    for key in keys {
        common::set_env(key, "outside-fixture");
    }
    let isolated = {
        let fixture = common::Fixture::new("034");
        assert!(fixture.home().is_dir());
        keys.iter().all(|key| std::env::var_os(key).is_none())
    };
    let restored = keys
        .iter()
        .all(|key| std::env::var_os(key).unwrap() == "outside-fixture");
    for (key, value) in saved {
        match value {
            Some(value) => common::set_env_os(key, value),
            None => common::remove_env(key),
        }
    }
    assert!(isolated, "agent override escaped fixture isolation");
    assert!(restored, "fixture did not restore overrides");
}

#[test]
fn case_034_paths() {
    common::with_fixture("034-paths", |f| {
        use rustcodegraph::installer::targets::shared::config_dir_from_env;
        let fallback = f.home().join("fallback");
        assert_eq!(config_dir_from_env("QWEN_HOME", &fallback), fallback);
        common::set_env("QWEN_HOME", "   ");
        assert_eq!(config_dir_from_env("QWEN_HOME", &fallback), fallback);
        common::set_env("QWEN_HOME", "relative");
        // Resolving against cwd does not canonicalize Windows short paths or add a verbatim prefix.
        assert_eq!(
            config_dir_from_env("QWEN_HOME", &fallback),
            std::env::current_dir().unwrap().join("relative")
        );
        common::set_env("QWEN_HOME", "~/config");
        assert_eq!(
            config_dir_from_env("QWEN_HOME", &fallback),
            f.home().join("config")
        );
        common::set_env("QWEN_HOME", &fallback);
        assert_eq!(config_dir_from_env("QWEN_HOME", &fallback), fallback);
    });
}
