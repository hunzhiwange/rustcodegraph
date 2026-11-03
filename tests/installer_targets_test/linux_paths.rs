use super::common::*;

#[test]
fn case_060_linux_paths_and_roundtrips() {
    with_fixture("060", |f| {
        for (id, suffix) in [
            ("copilot-vscode", ".config/Code/User/mcp.json"),
            ("copilot-cli", ".copilot/mcp-config.json"),
            (
                "copilot-jetbrains",
                ".config/github-copilot/intellij/mcp.json",
            ),
            ("bob", ".bob/settings/mcp.json"),
            ("droid", ".factory/mcp.json"),
            ("windsurf", ".codeium/windsurf/mcp_config.json"),
            ("qwen", ".qwen/settings.json"),
            ("dsh", ".dsh/cordis.patch.yml"),
            ("grok", ".grok/config.toml"),
            ("kimi", ".kimi-code/mcp.json"),
            ("qoder", ".qoder/settings.json"),
            ("pi", ".pi/agent/mcp.json"),
        ] {
            let t = get_target(id).unwrap();
            let p = f.home().join(suffix);
            assert_eq!(
                PathBuf::from(&t.describe_paths(Location::Global)[0]),
                p,
                "{id}"
            );
            assert!(!t.detect(Location::Global).already_configured, "{id}");
            assert!(!p.exists(), "{id}: detection must not write");
            t.install(Location::Global, install_options(false));
            assert!(t.detect(Location::Global).already_configured, "{id}");
            let before = fs::read(&p).unwrap();
            t.install(Location::Global, install_options(false));
            assert_eq!(before, fs::read(&p).unwrap(), "{id}: repeat install");
            t.uninstall(Location::Global);
            assert!(!t.detect(Location::Global).already_configured, "{id}");
            let after = fs::read(&p).ok();
            t.uninstall(Location::Global);
            assert_eq!(after, fs::read(&p).ok(), "{id}: absent uninstall");
        }
    });
}
