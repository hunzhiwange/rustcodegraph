use super::common::*;

#[test]
fn claude_local_install_writes_mcp_json_project_scope_not_claude_json() {
    with_fixture("partial-claude-local-mcp", |fixture| {
        let claude = target("claude");
        let result = claude.install(Location::Local, install_options(false));
        assert!(
            result
                .files
                .iter()
                .any(|file| normalize(&file.path).ends_with("/.mcp.json"))
        );
        assert!(fixture.cwd().join(".mcp.json").exists());
        assert!(!fixture.cwd().join(".claude.json").exists());
        assert!(
            read_json(fixture.cwd().join(".mcp.json"))
                .pointer("/mcpServers/rustcodegraph")
                .is_some()
        );
    });
}

#[test]
fn claude_install_creates_claude_md_codegraph_block_704() {
    with_fixture("partial-claude-md", |fixture| {
        let claude = target("claude");
        let result = claude.install(Location::Local, install_options(false));
        let claude_md = fixture.cwd().join(".claude").join("CLAUDE.md");
        assert!(claude_md.exists());
        let body = read_text(&claude_md);
        assert!(body.contains("## RustCodeGraph"));
        assert!(body.contains("rustcodegraph explore"));
        assert_eq!(
            find_file(&result.files, "CLAUDE.md").unwrap().action,
            WriteAction::Created
        );
    });
}

#[test]
fn claude_install_leaves_codegraph_block_and_appends_rustcodegraph_block() {
    with_fixture("partial-claude-md-legacy", |fixture| {
        let claude = target("claude");
        let claude_md = fixture.cwd().join(".claude").join("CLAUDE.md");
        write_text(
            &claude_md,
            &format!("# My project rules\n\nUse tabs.\n\n{LEGACY_BLOCK}\n"),
        );
        let result = claude.install(Location::Local, install_options(false));
        let body = read_text(&claude_md);
        assert!(body.contains("# My project rules"));
        assert!(body.contains("Use tabs."));
        assert!(body.contains("Prefer `codegraph_search`"));
        assert!(body.contains("CODEGRAPH_START"));
        assert!(body.contains("rustcodegraph explore"));
        assert_eq!(
            find_file(&result.files, "CLAUDE.md").unwrap().action,
            WriteAction::Updated
        );
    });
}

#[test]
fn claude_global_install_targets_home_claude_json_user_scope() {
    with_fixture("partial-claude-global", |fixture| {
        let claude = target("claude");
        claude.install(Location::Global, install_options(false));
        assert!(
            read_json(fixture.home().join(".claude.json"))
                .pointer("/mcpServers/rustcodegraph")
                .is_some()
        );
    });
}

#[test]
fn claude_local_install_migrates_legacy_project_claude_json_entry_into_mcp_json() {
    with_fixture("partial-claude-migrate", |fixture| {
        let claude = target("claude");
        let legacy = fixture.cwd().join(".claude.json");
        write_json(
            &legacy,
            json!({ "mcpServers": { "rustcodegraph": { "type": "stdio", "command": "rustcodegraph", "args": ["serve", "--mcp"] } } }),
        );
        claude.install(Location::Local, install_options(false));
        assert!(
            read_json(fixture.cwd().join(".mcp.json"))
                .pointer("/mcpServers/rustcodegraph")
                .is_some()
        );
        assert!(!legacy.exists());
    });
}

#[test]
fn claude_legacy_claude_json_migration_preserves_sibling_servers_and_unrelated_keys() {
    with_fixture("partial-claude-migrate-siblings", |fixture| {
        let claude = target("claude");
        let legacy = fixture.cwd().join(".claude.json");
        write_json(
            &legacy,
            json!({
                "mcpServers": {
                    "rustcodegraph": { "type": "stdio", "command": "rustcodegraph", "args": ["serve", "--mcp"] },
                    "other": { "command": "x" }
                },
                "somethingElse": true
            }),
        );
        claude.install(Location::Local, install_options(false));
        let after = read_json(&legacy);
        assert!(after.pointer("/mcpServers/rustcodegraph").is_none());
        assert!(after.pointer("/mcpServers/other").is_some());
        assert_eq!(after.pointer("/somethingElse").unwrap(), true);
        assert!(
            read_json(fixture.cwd().join(".mcp.json"))
                .pointer("/mcpServers/rustcodegraph")
                .is_some()
        );
    });
}

#[test]
fn claude_uninstall_strips_rustcodegraph_from_mcp_json_and_legacy_claude_json() {
    with_fixture("partial-claude-uninstall-legacy", |fixture| {
        let claude = target("claude");
        write_json(
            fixture.cwd().join(".mcp.json"),
            json!({ "mcpServers": { "rustcodegraph": { "command": "rustcodegraph" } } }),
        );
        write_json(
            fixture.cwd().join(".claude.json"),
            json!({ "mcpServers": { "rustcodegraph": { "command": "rustcodegraph" }, "other": { "command": "x" } } }),
        );
        claude.uninstall(Location::Local);
        assert!(
            read_json(fixture.cwd().join(".mcp.json"))
                .get("mcpServers")
                .is_none()
        );
        let legacy = read_json(fixture.cwd().join(".claude.json"));
        assert!(legacy.pointer("/mcpServers/rustcodegraph").is_none());
        assert!(legacy.pointer("/mcpServers/other").is_some());
    });
}

fn seed_settings(base: &Path, loc: Location, settings: Value) -> PathBuf {
    let dir = match loc {
        Location::Global => base.join(".claude"),
        Location::Local => env::current_dir().unwrap().join(".claude"),
    };
    let file = dir.join("settings.json");
    write_json(&file, settings);
    file
}

fn legacy_hook_settings() -> Value {
    json!({
        "hooks": {
            "PostToolUse": [
                { "matcher": "Edit|Write", "hooks": [{ "type": "command", "command": "rustcodegraph mark-dirty", "async": true }] }
            ],
            "Stop": [
                { "hooks": [{ "type": "command", "command": "rustcodegraph sync-if-dirty" }] },
                { "hooks": [{ "type": "command", "command": "\"/Users/me/gk\" ai hook run --host claude-code" }] }
            ]
        }
    })
}

#[test]
fn claude_install_strips_stale_auto_sync_hooks_but_keeps_user_hook() {
    with_fixture("partial-claude-hooks-install", |fixture| {
        let claude = target("claude");
        let file = seed_settings(fixture.home(), Location::Global, legacy_hook_settings());
        claude.install(Location::Global, install_options(true));
        let after = read_json(&file);
        assert!(after.pointer("/hooks/PostToolUse").is_none());
        let stop_commands = after
            .pointer("/hooks/Stop")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .flat_map(|group| group["hooks"].as_array().unwrap().iter())
            .map(|hook| hook["command"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert!(
            !stop_commands
                .iter()
                .any(|cmd| cmd == "rustcodegraph sync-if-dirty")
        );
        assert!(
            stop_commands
                .iter()
                .any(|cmd| cmd.contains("gk") && cmd.contains("ai hook run"))
        );
        assert!(
            after
                .pointer("/permissions/allow")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value.as_str() == Some("mcp__rustcodegraph__rustcodegraph_search"))
        );
    });
}

#[test]
fn claude_cleanup_legacy_hooks_preserves_sibling_hook_sharing_matcher_group() {
    with_fixture("partial-claude-hooks-sibling", |fixture| {
        let file = seed_settings(
            fixture.home(),
            Location::Global,
            json!({
                "hooks": {
                    "Stop": [{
                        "hooks": [
                            { "type": "command", "command": "rustcodegraph sync-if-dirty" },
                            { "type": "command", "command": "gk ai hook run --host claude-code" }
                        ]
                    }]
                }
            }),
        );
        assert_eq!(
            cleanup_legacy_hooks(Location::Global).action,
            WriteAction::Removed
        );
        let after = read_json(&file);
        let commands = after
            .pointer("/hooks/Stop/0/hooks")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|hook| hook["command"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(commands, vec!["gk ai hook run --host claude-code"]);
    });
}

#[test]
fn claude_cleanup_legacy_hooks_is_byte_for_byte_noop_without_rustcodegraph_hooks() {
    with_fixture("partial-claude-hooks-noop", |fixture| {
        let original = serde_json::to_string_pretty(&json!({
            "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "gk ai hook run" }] }] }
        }))
        .unwrap()
            + "\n";
        let file = fixture.home().join(".claude").join("settings.json");
        write_text(&file, &original);
        assert_eq!(
            cleanup_legacy_hooks(Location::Global).action,
            WriteAction::Unchanged
        );
        assert_eq!(read_text(&file), original);
    });
}

#[test]
fn claude_cleanup_legacy_hooks_reports_not_found_when_settings_json_absent() {
    with_fixture("partial-claude-hooks-absent", |_| {
        assert_eq!(
            cleanup_legacy_hooks(Location::Global).action,
            WriteAction::NotFound
        );
    });
}

#[test]
fn claude_re_running_install_after_legacy_cleanup_leaves_settings_json_unchanged() {
    with_fixture("partial-claude-hooks-rerun", |fixture| {
        let claude = target("claude");
        let file = seed_settings(fixture.home(), Location::Global, legacy_hook_settings());
        claude.install(Location::Global, install_options(true));
        let first_pass = read_text(&file);
        claude.install(Location::Global, install_options(true));
        assert_eq!(read_text(&file), first_pass);
    });
}

#[test]
fn claude_uninstall_does_not_strip_stale_codegraph_npx_hooks() {
    with_fixture("partial-claude-hooks-local", |_| {
        let claude = target("claude");
        let file = seed_settings(
            Path::new("unused"),
            Location::Local,
            json!({
                "hooks": {
                    "PostToolUse": [
                        { "matcher": "Edit|Write", "hooks": [{ "type": "command", "command": "npx rustcodegraph mark-dirty", "async": true }] }
                    ],
                    "Stop": [
                        { "hooks": [{ "type": "command", "command": "npx rustcodegraph sync-if-dirty" }] }
                    ]
                }
            }),
        );
        claude.uninstall(Location::Local);
        let after = read_json(&file);
        assert!(after.pointer("/hooks/PostToolUse").is_some());
        assert!(after.pointer("/hooks/Stop").is_some());
    });
}
