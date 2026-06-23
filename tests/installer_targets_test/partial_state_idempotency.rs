use super::common::*;

#[test]
fn codex_install_writes_config_toml_and_agents_md_codegraph_block_704() {
    with_fixture("partial-codex-write", |fixture| {
        let codex = target("codex");
        let first = codex.install(Location::Global, install_options(false));
        let agents_md = fixture.home().join(".codex").join("AGENTS.md");
        assert!(
            first
                .files
                .iter()
                .any(|file| file.path.ends_with("config.toml"))
        );
        assert!(agents_md.exists());
        let body = read_text(&agents_md);
        assert!(body.contains("## RustCodeGraph"));
        assert!(body.contains("rustcodegraph explore"));
        let second = codex.install(Location::Global, install_options(false));
        for file in second.files {
            assert_eq!(file.action, WriteAction::Unchanged);
        }
    });
}

#[test]
fn codex_install_leaves_codegraph_block_and_appends_rustcodegraph_block() {
    with_fixture("partial-codex-legacy", |fixture| {
        let codex = target("codex");
        let agents_md = fixture.home().join(".codex").join("AGENTS.md");
        write_text(
            &agents_md,
            &format!("# My codex notes\n\nBe terse.\n\n{LEGACY_BLOCK}\n"),
        );
        let result = codex.install(Location::Global, install_options(false));
        let body = read_text(&agents_md);
        assert!(body.contains("# My codex notes"));
        assert!(body.contains("Be terse."));
        assert!(body.contains("Prefer `codegraph_search`"));
        assert!(body.contains("CODEGRAPH_START"));
        assert!(body.contains("rustcodegraph explore"));
        assert_eq!(
            find_file(&result.files, "AGENTS.md").unwrap().action,
            WriteAction::Updated
        );
    });
}

#[test]
fn opencode_prefers_jsonc_when_both_json_and_jsonc_exist() {
    with_fixture("partial-opencode-jsonc", |fixture| {
        let opencode = target("opencode");
        let dir = fixture.home().join(".config").join("opencode");
        write_text(
            dir.join("opencode.json"),
            "{\n  \"$schema\": \"https://opencode.ai/config.json\"\n}\n",
        );
        write_text(
            dir.join("opencode.jsonc"),
            "{\n  \"$schema\": \"https://opencode.ai/config.json\"\n}\n",
        );
        let result = opencode.install(Location::Global, install_options(true));
        let written = result
            .files
            .iter()
            .find(|file| file.path.ends_with(".jsonc"))
            .unwrap();
        assert_ne!(written.action, WriteAction::NotFound);
        assert!(!read_text(dir.join("opencode.json")).contains("rustcodegraph"));
    });
}

#[test]
fn opencode_uses_json_when_only_json_exists_no_jsonc() {
    with_fixture("partial-opencode-json", |fixture| {
        let opencode = target("opencode");
        let dir = fixture.home().join(".config").join("opencode");
        write_text(
            dir.join("opencode.json"),
            "{\n  \"$schema\": \"https://opencode.ai/config.json\"\n}\n",
        );
        let result = opencode.install(Location::Global, install_options(true));
        assert!(result.files[0].path.ends_with("opencode.json"));
        assert!(!dir.join("opencode.jsonc").exists());
    });
}

#[test]
fn opencode_defaults_to_jsonc_for_fresh_installs_no_existing_file() {
    with_fixture("partial-opencode-fresh", |_| {
        let opencode = target("opencode");
        let result = opencode.install(Location::Global, install_options(true));
        assert!(result.files[0].path.ends_with("opencode.jsonc"));
        assert_eq!(result.files[0].action, WriteAction::Created);
    });
}

#[test]
fn opencode_preserves_line_and_block_comments_through_install_and_idempotent_rerun() {
    with_fixture("partial-opencode-comments", |fixture| {
        let opencode = target("opencode");
        let file = fixture
            .home()
            .join(".config")
            .join("opencode")
            .join("opencode.jsonc");
        let original = [
            "{",
            "  // top-level note about my opencode setup",
            "  \"$schema\": \"https://opencode.ai/config.json\",",
            "  /* multi-line block comment",
            "     describing the providers section */",
            "  \"providers\": {",
            "    \"anthropic\": { \"model\": \"claude-opus-4-7\" } // pinned",
            "  }",
            "}",
            "",
        ]
        .join("\n");
        write_text(&file, &original);
        opencode.install(Location::Global, install_options(true));
        let after_install = read_text(&file);
        assert!(after_install.contains("// top-level note about my opencode setup"));
        assert!(after_install.contains("/* multi-line block comment"));
        assert!(after_install.contains("// pinned"));
        assert!(after_install.contains("\"rustcodegraph\""));
        assert!(after_install.contains("\"providers\""));
        let second = opencode.install(Location::Global, install_options(true));
        assert_eq!(second.files[0].action, WriteAction::Unchanged);
        assert_eq!(read_text(&file), after_install);
    });
}

#[test]
fn opencode_install_writes_agents_md_codegraph_block_704() {
    with_fixture("partial-opencode-agents", |fixture| {
        let opencode = target("opencode");
        let result = opencode.install(Location::Global, install_options(true));
        let agents_md = fixture
            .home()
            .join(".config")
            .join("opencode")
            .join("AGENTS.md");
        assert!(agents_md.exists());
        assert!(read_text(&agents_md).contains("rustcodegraph explore"));
        assert_eq!(
            find_file(&result.files, "AGENTS.md").unwrap().action,
            WriteAction::Created
        );
    });
}

#[test]
fn opencode_install_leaves_codegraph_block_and_appends_rustcodegraph_block() {
    with_fixture("partial-opencode-legacy", |fixture| {
        let opencode = target("opencode");
        let agents_md = fixture
            .home()
            .join(".config")
            .join("opencode")
            .join("AGENTS.md");
        write_text(
            &agents_md,
            &format!(
                "# My personal opencode instructions\n\nAlways respond directly.\n\n{LEGACY_BLOCK}\n"
            ),
        );
        let result = opencode.install(Location::Global, install_options(true));
        let body = read_text(&agents_md);
        assert!(body.contains("# My personal opencode instructions"));
        assert!(body.contains("Always respond directly."));
        assert!(body.contains("Prefer `codegraph_search`"));
        assert!(body.contains("CODEGRAPH_START"));
        assert!(body.contains("rustcodegraph explore"));
        assert_eq!(
            find_file(&result.files, "AGENTS.md").unwrap().action,
            WriteAction::Updated
        );
    });
}

#[test]
fn opencode_uninstall_leaves_codegraph_block_and_user_content() {
    with_fixture("partial-opencode-uninstall-agents", |fixture| {
        let opencode = target("opencode");
        let agents_md = fixture
            .home()
            .join(".config")
            .join("opencode")
            .join("AGENTS.md");
        write_text(
            &agents_md,
            &format!(
                "# My personal opencode instructions\n\nAlways respond directly.\n\n{LEGACY_BLOCK}\n"
            ),
        );
        opencode.uninstall(Location::Global);
        let body = read_text(&agents_md);
        assert!(body.contains("# My personal opencode instructions"));
        assert!(body.contains("Always respond directly."));
        assert!(body.contains("CODEGRAPH_START"));
    });
}

#[test]
fn opencode_local_install_writes_opencode_jsonc_and_agents_md_block_704() {
    with_fixture("partial-opencode-local", |fixture| {
        let opencode = target("opencode");
        let result = opencode.install(Location::Local, install_options(true));
        let paths = result
            .files
            .iter()
            .map(|file| normalize(&file.path))
            .collect::<Vec<_>>();
        assert!(paths.iter().any(|path| path.ends_with("/opencode.jsonc")));
        assert!(paths.iter().any(|path| path.ends_with("/AGENTS.md")));
        assert!(fixture.cwd().join("AGENTS.md").exists());
    });
}

#[test]
fn gemini_install_writes_settings_json_and_gemini_md_block_704() {
    with_fixture("partial-gemini-write", |fixture| {
        let gemini = target("gemini");
        let result = gemini.install(Location::Global, install_options(true));
        let settings = fixture.home().join(".gemini").join("settings.json");
        let gemini_md = fixture.home().join(".gemini").join("GEMINI.md");
        assert!(
            result
                .files
                .iter()
                .any(|file| Path::new(&file.path) == settings)
        );
        assert!(
            result
                .files
                .iter()
                .any(|file| Path::new(&file.path) == gemini_md)
        );
        assert!(gemini_md.exists());
        assert!(read_text(&gemini_md).contains("rustcodegraph explore"));
        let cfg = read_json(&settings);
        assert_eq!(
            cfg.pointer("/mcpServers/rustcodegraph").unwrap(),
            &json!({ "type": "stdio", "command": "rustcodegraph", "args": ["serve", "--mcp"] })
        );
    });
}

#[test]
fn gemini_install_preserves_pre_existing_settings_security_auth_survives() {
    with_fixture("partial-gemini-preserve", |fixture| {
        let gemini = target("gemini");
        let settings = fixture.home().join(".gemini").join("settings.json");
        write_json(
            &settings,
            json!({ "security": { "auth": { "selectedType": "oauth-personal" } } }),
        );
        gemini.install(Location::Global, install_options(true));
        let after = read_json(&settings);
        assert_eq!(
            after.pointer("/security/auth/selectedType").unwrap(),
            "oauth-personal"
        );
        assert!(after.pointer("/mcpServers/rustcodegraph").is_some());
    });
}

#[test]
fn gemini_uninstall_strips_rustcodegraph_but_leaves_pre_existing_settings_intact() {
    with_fixture("partial-gemini-uninstall", |fixture| {
        let gemini = target("gemini");
        let settings = fixture.home().join(".gemini").join("settings.json");
        write_json(
            &settings,
            json!({ "security": { "auth": { "selectedType": "oauth-personal" } } }),
        );
        gemini.install(Location::Global, install_options(true));
        gemini.uninstall(Location::Global);
        let after = read_json(&settings);
        assert_eq!(
            after.pointer("/security/auth/selectedType").unwrap(),
            "oauth-personal"
        );
        assert!(after.get("mcpServers").is_none());
    });
}

#[test]
fn gemini_local_install_writes_project_settings_and_project_root_gemini_md_block_704() {
    with_fixture("partial-gemini-local", |fixture| {
        let gemini = target("gemini");
        let result = gemini.install(Location::Local, install_options(true));
        let paths = result
            .files
            .iter()
            .map(|file| normalize(&file.path))
            .collect::<Vec<_>>();
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with("/.gemini/settings.json"))
        );
        assert!(paths.iter().any(|path| path.ends_with("/GEMINI.md")));
        assert!(fixture.cwd().join("GEMINI.md").exists());
    });
}

#[test]
fn gemini_uninstall_leaves_codegraph_block_and_user_content() {
    with_fixture("partial-gemini-md-uninstall", |fixture| {
        let gemini = target("gemini");
        let gemini_md = fixture.home().join(".gemini").join("GEMINI.md");
        write_text(
            &gemini_md,
            &format!(
                "# My personal Gemini context\n\nAlways respond concisely.\n\n{LEGACY_BLOCK}\n"
            ),
        );
        gemini.uninstall(Location::Global);
        let body = read_text(&gemini_md);
        assert!(body.contains("# My personal Gemini context"));
        assert!(body.contains("Always respond concisely."));
        assert!(body.contains("CODEGRAPH_START"));
    });
}

#[test]
fn kiro_install_writes_settings_mcp_json_and_no_steering_doc_529() {
    with_fixture("partial-kiro-write", |fixture| {
        let kiro = target("kiro");
        let result = kiro.install(Location::Global, install_options(true));
        let mcp = fixture
            .home()
            .join(".kiro")
            .join("settings")
            .join("mcp.json");
        let steering = fixture
            .home()
            .join(".kiro")
            .join("steering")
            .join("rustcodegraph.md");
        assert!(result.files.iter().any(|file| Path::new(&file.path) == mcp));
        assert!(
            !result
                .files
                .iter()
                .any(|file| Path::new(&file.path) == steering)
        );
        assert!(!steering.exists());
        let cfg = read_json(&mcp);
        assert_eq!(
            cfg.pointer("/mcpServers/rustcodegraph").unwrap(),
            &json!({ "type": "stdio", "command": "rustcodegraph", "args": ["serve", "--mcp"] })
        );
    });
}

#[test]
fn kiro_install_deletes_leftover_steering_doc_self_heal_529() {
    with_fixture("partial-kiro-steering-install", |fixture| {
        let kiro = target("kiro");
        let steering = fixture
            .home()
            .join(".kiro")
            .join("steering")
            .join("rustcodegraph.md");
        write_text(&steering, &format!("{LEGACY_BLOCK}\n"));
        let result = kiro.install(Location::Global, install_options(true));
        assert!(!steering.exists());
        assert_eq!(
            find_file(&result.files, "rustcodegraph.md").unwrap().action,
            WriteAction::Removed
        );
    });
}

#[test]
fn kiro_install_preserves_pre_existing_sibling_mcp_server_in_mcp_json() {
    with_fixture("partial-kiro-sibling", |fixture| {
        let kiro = target("kiro");
        let mcp = fixture
            .home()
            .join(".kiro")
            .join("settings")
            .join("mcp.json");
        write_json(
            &mcp,
            json!({ "mcpServers": { "other": { "command": "uvx", "args": ["other-server"] } } }),
        );
        kiro.install(Location::Global, install_options(true));
        let after = read_json(&mcp);
        assert!(after.pointer("/mcpServers/other").is_some());
        assert!(after.pointer("/mcpServers/rustcodegraph").is_some());
    });
}

#[test]
fn kiro_uninstall_strips_rustcodegraph_but_leaves_sibling_mcp_servers_intact() {
    with_fixture("partial-kiro-uninstall-sibling", |fixture| {
        let kiro = target("kiro");
        let mcp = fixture
            .home()
            .join(".kiro")
            .join("settings")
            .join("mcp.json");
        write_json(
            &mcp,
            json!({ "mcpServers": { "other": { "command": "uvx", "args": ["other-server"] } } }),
        );
        kiro.install(Location::Global, install_options(true));
        kiro.uninstall(Location::Global);
        let after = read_json(&mcp);
        assert!(after.pointer("/mcpServers/other").is_some());
        assert!(after.pointer("/mcpServers/rustcodegraph").is_none());
    });
}

#[test]
fn kiro_uninstall_removes_leftover_steering_doc_file_outright() {
    with_fixture("partial-kiro-steering-uninstall", |fixture| {
        let kiro = target("kiro");
        let steering = fixture
            .home()
            .join(".kiro")
            .join("steering")
            .join("rustcodegraph.md");
        write_text(&steering, &format!("{LEGACY_BLOCK}\n"));
        kiro.uninstall(Location::Global);
        assert!(!steering.exists());
    });
}

#[test]
fn kiro_uninstall_removes_our_steering_doc_but_leaves_sibling_untouched() {
    with_fixture("partial-kiro-steering-sibling", |fixture| {
        let kiro = target("kiro");
        let sibling = fixture
            .home()
            .join(".kiro")
            .join("steering")
            .join("product.md");
        let ours = fixture
            .home()
            .join(".kiro")
            .join("steering")
            .join("rustcodegraph.md");
        write_text(&sibling, "# Product\n\nMy team practices.\n");
        write_text(&ours, &format!("{LEGACY_BLOCK}\n"));
        kiro.uninstall(Location::Global);
        assert!(!ours.exists());
        assert!(sibling.exists());
        assert!(read_text(&sibling).contains("My team practices."));
    });
}

#[test]
fn kiro_local_install_writes_project_mcp_json_and_no_steering_doc_529() {
    with_fixture("partial-kiro-local", |_| {
        let kiro = target("kiro");
        let result = kiro.install(Location::Local, install_options(true));
        let paths = result
            .files
            .iter()
            .map(|file| normalize(&file.path))
            .collect::<Vec<_>>();
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with("/.kiro/settings/mcp.json"))
        );
        assert!(
            !paths
                .iter()
                .any(|path| path.ends_with("/.kiro/steering/rustcodegraph.md"))
        );
    });
}
