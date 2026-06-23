use super::common::*;

#[test]
fn antigravity_install_writes_to_legacy_path_when_no_migration_marker() {
    with_fixture("partial-antigravity-legacy", |fixture| {
        let antigravity = target("antigravity");
        antigravity.install(Location::Global, install_options(true));
        let legacy_file = fixture
            .home()
            .join(".gemini")
            .join("antigravity")
            .join("mcp_config.json");
        assert!(legacy_file.exists());
        assert!(
            read_json(&legacy_file)
                .pointer("/mcpServers/rustcodegraph")
                .is_some()
        );
        assert!(
            !fixture
                .home()
                .join(".gemini")
                .join("settings.json")
                .exists()
        );
    });
}

#[test]
fn antigravity_install_writes_to_unified_path_when_migrated_marker_present() {
    with_fixture("partial-antigravity-unified-marker", |fixture| {
        let antigravity = target("antigravity");
        let unified_dir = fixture.home().join(".gemini").join("config");
        write_text(unified_dir.join(".migrated"), "");
        antigravity.install(Location::Global, install_options(true));
        let unified_file = unified_dir.join("mcp_config.json");
        assert!(unified_file.exists());
        assert!(
            read_json(&unified_file)
                .pointer("/mcpServers/rustcodegraph")
                .is_some()
        );
        assert!(
            !fixture
                .home()
                .join(".gemini")
                .join("antigravity")
                .join("mcp_config.json")
                .exists()
        );
    });
}

#[test]
fn antigravity_install_writes_to_unified_path_when_unified_file_already_exists() {
    with_fixture("partial-antigravity-unified-existing", |fixture| {
        let antigravity = target("antigravity");
        let unified_file = fixture
            .home()
            .join(".gemini")
            .join("config")
            .join("mcp_config.json");
        write_json(&unified_file, json!({ "mcpServers": {} }));
        antigravity.install(Location::Global, install_options(true));
        assert!(
            read_json(&unified_file)
                .pointer("/mcpServers/rustcodegraph")
                .is_some()
        );
    });
}

#[test]
fn antigravity_entry_has_no_type_field() {
    with_fixture("partial-antigravity-no-type", |fixture| {
        let antigravity = target("antigravity");
        let unified_dir = fixture.home().join(".gemini").join("config");
        write_text(unified_dir.join(".migrated"), "");
        antigravity.install(Location::Global, install_options(true));
        let cfg = read_json(unified_dir.join("mcp_config.json"));
        assert!(cfg.pointer("/mcpServers/rustcodegraph/type").is_none());
        assert!(cfg.pointer("/mcpServers/rustcodegraph/command").is_some());
        assert_eq!(
            cfg.pointer("/mcpServers/rustcodegraph/args").unwrap(),
            &json!(["serve", "--mcp"])
        );
    });
}

#[test]
fn antigravity_install_migrates_legacy_rustcodegraph_entry_to_unified_path_when_marker_appears() {
    with_fixture("partial-antigravity-migrate", |fixture| {
        let antigravity = target("antigravity");
        let legacy_file = fixture
            .home()
            .join(".gemini")
            .join("antigravity")
            .join("mcp_config.json");
        write_json(
            &legacy_file,
            json!({ "mcpServers": { "rustcodegraph": { "command": "rustcodegraph", "args": ["serve", "--mcp"] } } }),
        );
        let unified_dir = fixture.home().join(".gemini").join("config");
        write_text(unified_dir.join(".migrated"), "");
        antigravity.install(Location::Global, install_options(true));
        assert!(
            read_json(unified_dir.join("mcp_config.json"))
                .pointer("/mcpServers/rustcodegraph")
                .is_some()
        );
        assert!(read_json(&legacy_file).get("mcpServers").is_none());
    });
}

#[test]
fn antigravity_install_preserves_sibling_mcp_server_in_legacy_path() {
    with_fixture("partial-antigravity-sibling", |fixture| {
        let antigravity = target("antigravity");
        let mcp_file = fixture
            .home()
            .join(".gemini")
            .join("antigravity")
            .join("mcp_config.json");
        write_json(
            &mcp_file,
            json!({ "mcpServers": { "other": { "command": "uvx", "args": ["other-server"] } } }),
        );
        antigravity.install(Location::Global, install_options(true));
        let after = read_json(&mcp_file);
        assert!(after.pointer("/mcpServers/other").is_some());
        assert!(after.pointer("/mcpServers/rustcodegraph").is_some());
    });
}

#[test]
fn antigravity_install_preserves_managed_fields_on_sibling_servers() {
    with_fixture("partial-antigravity-disabled", |fixture| {
        let antigravity = target("antigravity");
        let unified_dir = fixture.home().join(".gemini").join("config");
        write_text(unified_dir.join(".migrated"), "");
        let unified = unified_dir.join("mcp_config.json");
        write_json(
            &unified,
            json!({
                "mcpServers": {
                    "code-review-graph": {
                        "command": "uvx",
                        "args": ["code-review-graph", "serve"],
                        "disabled": true
                    }
                }
            }),
        );
        antigravity.install(Location::Global, install_options(true));
        let after = read_json(&unified);
        assert_eq!(
            after
                .pointer("/mcpServers/code-review-graph/disabled")
                .unwrap(),
            true
        );
        assert!(after.pointer("/mcpServers/rustcodegraph").is_some());
    });
}

#[test]
fn antigravity_uninstall_removes_only_rustcodegraph_sibling_survives() {
    with_fixture("partial-antigravity-uninstall-sibling", |fixture| {
        let antigravity = target("antigravity");
        let mcp_file = fixture
            .home()
            .join(".gemini")
            .join("antigravity")
            .join("mcp_config.json");
        write_json(
            &mcp_file,
            json!({ "mcpServers": { "other": { "command": "uvx", "args": ["other-server"] } } }),
        );
        antigravity.install(Location::Global, install_options(true));
        antigravity.uninstall(Location::Global);
        let after = read_json(&mcp_file);
        assert!(after.pointer("/mcpServers/other").is_some());
        assert!(after.pointer("/mcpServers/rustcodegraph").is_none());
    });
}

#[test]
fn antigravity_uninstall_sweeps_both_legacy_and_unified_paths() {
    with_fixture("partial-antigravity-uninstall-both", |fixture| {
        let antigravity = target("antigravity");
        let legacy = fixture
            .home()
            .join(".gemini")
            .join("antigravity")
            .join("mcp_config.json");
        let unified = fixture
            .home()
            .join(".gemini")
            .join("config")
            .join("mcp_config.json");
        write_json(
            &legacy,
            json!({ "mcpServers": { "rustcodegraph": { "command": "rustcodegraph", "args": ["serve", "--mcp"] } } }),
        );
        write_json(
            &unified,
            json!({ "mcpServers": { "rustcodegraph": { "command": "rustcodegraph", "args": ["serve", "--mcp"] } } }),
        );
        write_text(unified.parent().unwrap().join(".migrated"), "");
        antigravity.uninstall(Location::Global);
        assert!(read_json(&legacy).get("mcpServers").is_none());
        assert!(read_json(&unified).get("mcpServers").is_none());
    });
}

#[test]
fn antigravity_rejects_local_location_with_clear_note() {
    with_fixture("partial-antigravity-local", |_| {
        let antigravity = target("antigravity");
        assert!(!antigravity.supports_location(Location::Local));
        let result = antigravity.install(Location::Local, install_options(true));
        assert!(result.files.is_empty());
        assert!(
            result
                .notes
                .unwrap()
                .join(" ")
                .contains("no project-local config")
        );
    });
}

#[test]
fn antigravity_does_not_write_gemini_md() {
    with_fixture("partial-antigravity-no-gemini-md", |fixture| {
        let antigravity = target("antigravity");
        antigravity.install(Location::Global, install_options(true));
        assert!(!fixture.home().join(".gemini").join("GEMINI.md").exists());
    });
}

#[test]
fn gemini_and_antigravity_both_installed_coexist() {
    with_fixture("partial-gemini-antigravity", |fixture| {
        let gemini = target("gemini");
        let antigravity = target("antigravity");
        gemini.install(Location::Global, install_options(true));
        antigravity.install(Location::Global, install_options(true));
        let cli_cfg = read_json(fixture.home().join(".gemini").join("settings.json"));
        let ide_cfg = read_json(
            fixture
                .home()
                .join(".gemini")
                .join("antigravity")
                .join("mcp_config.json"),
        );
        assert!(cli_cfg.pointer("/mcpServers/rustcodegraph").is_some());
        assert!(ide_cfg.pointer("/mcpServers/rustcodegraph").is_some());
        antigravity.uninstall(Location::Global);
        let cli_after = read_json(fixture.home().join(".gemini").join("settings.json"));
        assert!(cli_after.pointer("/mcpServers/rustcodegraph").is_some());
    });
}

#[test]
fn hermes_install_adds_mcp_server_and_cli_toolset_preserving_existing_yaml() {
    with_fixture("partial-hermes-write", |fixture| {
        let hermes = target("hermes");
        let config = fixture.home().join(".hermes").join("config.yaml");
        write_text(
            &config,
            &[
                "model:",
                "  default: qwen-3.7",
                "mcp_servers:",
                "  other:",
                "    command: other",
                "platform_toolsets:",
                "  cli:",
                "    - hermes-cli",
                "  discord:",
                "    - hermes-discord",
                "",
            ]
            .join("\n"),
        );
        let result = hermes.install(Location::Global, install_options(true));
        assert_eq!(result.files[0].action, WriteAction::Updated);
        let body = read_text(&config);
        assert!(body.contains("model:\n  default: qwen-3.7"));
        assert!(body.contains("mcp_servers:\n  other:\n    command: other"));
        assert!(body.contains("  rustcodegraph:\n    command: rustcodegraph"));
        assert!(body.contains("    - hermes-cli"));
        assert!(body.contains("    - mcp-rustcodegraph"));
        assert!(body.contains("  discord:\n    - hermes-discord"));
        let second = hermes.install(Location::Global, install_options(true));
        assert_eq!(second.files[0].action, WriteAction::Unchanged);
    });
}

#[test]
fn hermes_uninstall_removes_only_rustcodegraph_mcp_server_and_toolset_entry() {
    with_fixture("partial-hermes-uninstall", |fixture| {
        let hermes = target("hermes");
        let config = fixture.home().join(".hermes").join("config.yaml");
        hermes.install(Location::Global, install_options(true));
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&config)
            .expect("failed to open hermes config");
        file.write_all(b"custom:\n  keep: true\n")
            .expect("failed to append hermes config");
        drop(file);
        hermes.uninstall(Location::Global);
        let body = read_text(&config);
        assert!(!body.contains("rustcodegraph:"));
        assert!(!body.contains("mcp-rustcodegraph"));
        assert!(body.contains("custom:\n  keep: true"));
    });
}

#[test]
fn hermes_install_preserves_pyyaml_default_list_at_same_indent_style_456() {
    with_fixture("partial-hermes-pyyaml", |fixture| {
        let hermes = target("hermes");
        let config = fixture.home().join(".hermes").join("config.yaml");
        write_text(
            &config,
            &[
                "model:",
                "  default: gpt-4o",
                "platform_toolsets:",
                "  cli:",
                "  - hermes-cli",
                "  - browser",
                "  - clarify",
                "  - terminal",
                "  - web",
                "  telegram:",
                "  - hermes-telegram",
                "  discord:",
                "  - hermes-discord",
                "",
            ]
            .join("\n"),
        );
        hermes.install(Location::Global, install_options(true));
        let body = read_text(&config);
        assert!(body.contains("\n  - mcp-rustcodegraph\n"));
        assert!(body.contains("\n  - hermes-cli\n"));
        assert!(body.contains("\n  telegram:\n  - hermes-telegram\n"));
        assert!(body.contains("\n  discord:\n  - hermes-discord\n"));
        assert!(!body.lines().any(|line| line == "- browser"));
        assert!(!body.lines().any(|line| line == "- hermes-telegram"));
        assert!(body.contains("  cli:\n  - hermes-cli\n  - browser"));
        let second = hermes.install(Location::Global, install_options(true));
        assert_eq!(second.files[0].action, WriteAction::Unchanged);
    });
}

#[test]
fn hermes_uninstall_reverses_install_on_pyyaml_default_config() {
    with_fixture("partial-hermes-pyyaml-uninstall", |fixture| {
        let hermes = target("hermes");
        let config = fixture.home().join(".hermes").join("config.yaml");
        write_text(
            &config,
            &[
                "platform_toolsets:",
                "  cli:",
                "  - hermes-cli",
                "  - browser",
                "  telegram:",
                "  - hermes-telegram",
                "",
            ]
            .join("\n"),
        );
        hermes.install(Location::Global, install_options(true));
        let installed = read_text(&config);
        assert!(installed.contains("- mcp-rustcodegraph"));
        assert!(installed.contains("rustcodegraph:"));
        hermes.uninstall(Location::Global);
        let body = read_text(&config);
        assert!(!body.contains("mcp-rustcodegraph"));
        assert!(!body.contains("command: rustcodegraph"));
        assert!(body.contains("  cli:\n  - hermes-cli\n  - browser"));
        assert!(body.contains("  telegram:\n  - hermes-telegram"));
    });
}

#[test]
fn opencode_uninstall_removes_only_mcp_rustcodegraph_preserves_comments_and_siblings() {
    with_fixture("partial-opencode-uninstall-comments", |fixture| {
        let opencode = target("opencode");
        let file = fixture
            .home()
            .join(".config")
            .join("opencode")
            .join("opencode.jsonc");
        write_text(
            &file,
            &[
                "{",
                "  // important comment",
                "  \"$schema\": \"https://opencode.ai/config.json\",",
                "  \"mcp\": {",
                "    \"other\": { \"type\": \"local\", \"command\": [\"x\"], \"enabled\": true }",
                "  }",
                "}",
                "",
            ]
            .join("\n"),
        );
        opencode.install(Location::Global, install_options(true));
        let after_install = read_text(&file);
        assert!(after_install.contains("\"rustcodegraph\""));
        assert!(after_install.contains("\"other\""));
        opencode.uninstall(Location::Global);
        let after_uninstall = read_text(&file);
        assert!(!after_uninstall.contains("rustcodegraph"));
        assert!(after_uninstall.contains("// important comment"));
        assert!(after_uninstall.contains("\"other\""));
    });
}

#[test]
fn codex_user_added_key_inside_mcp_servers_block_is_removed_on_reinstall() {
    with_fixture("partial-codex-owned-block", |fixture| {
        let codex = target("codex");
        codex.install(Location::Global, install_options(false));
        let toml_path = fixture.home().join(".codex").join("config.toml");
        let original = read_text(&toml_path);
        write_text(
            &toml_path,
            &original.replace(
                "args = [\"serve\", \"--mcp\"]",
                "args = [\"serve\", \"--mcp\"]\nenabled = true",
            ),
        );
        let second = codex.install(Location::Global, install_options(false));
        assert_eq!(
            find_file(&second.files, "config.toml").unwrap().action,
            WriteAction::Updated
        );
        assert!(!read_text(&toml_path).contains("enabled = true"));
    });
}
