use super::common::*;

#[test]
fn install_writes_files_detect_already_configured_becomes_true() {
    with_fixture("contract-install", |_| {
        for target in all_targets() {
            for location in supported_locations(target.as_ref()) {
                assert!(
                    !target.detect(location).already_configured,
                    "{} {location:?} should start unconfigured",
                    target.id().as_str()
                );

                let result = target.install(location, install_options(true));
                assert!(
                    !result.files.is_empty(),
                    "{} {location:?} should report files",
                    target.id().as_str()
                );
                for file in &result.files {
                    if file.action != WriteAction::Unchanged && file.action != WriteAction::NotFound
                    {
                        assert!(
                            Path::new(&file.path).exists(),
                            "expected {} to exist",
                            file.path
                        );
                    }
                }
                assert!(
                    target.detect(location).already_configured,
                    "{} {location:?} should become configured",
                    target.id().as_str()
                );
                target.uninstall(location);
            }
        }
    });
}

#[test]
fn re_running_install_is_idempotent_no_actions_other_than_unchanged() {
    with_fixture("contract-idempotent", |_| {
        for target in all_targets() {
            for location in supported_locations(target.as_ref()) {
                target.install(location, install_options(true));
                let second = target.install(location, install_options(true));
                for file in &second.files {
                    assert_eq!(
                        file.action,
                        WriteAction::Unchanged,
                        "{} {location:?} expected unchanged for {}",
                        target.id().as_str(),
                        file.path
                    );
                }
                target.uninstall(location);
            }
        }
    });
}

#[test]
fn install_preserves_a_pre_existing_sibling_mcp_server_where_applicable() {
    with_fixture("contract-sibling", |_| {
        for target in all_targets() {
            for location in supported_locations(target.as_ref()) {
                let paths = target.describe_paths(location);
                let Some(json_path) = paths
                    .iter()
                    .find(|path| path.ends_with(".json") || path.ends_with(".jsonc"))
                else {
                    continue;
                };
                let path = PathBuf::from(json_path);
                if target.id() == TargetId::Opencode {
                    write_json(
                        &path,
                        json!({ "mcp": { "other": { "type": "local", "command": ["x"], "enabled": true } } }),
                    );
                } else {
                    write_json(
                        &path,
                        json!({ "mcpServers": { "other": { "command": "x" } } }),
                    );
                }

                target.install(location, install_options(true));
                let after = read_json(&path);
                if target.id() == TargetId::Opencode {
                    assert!(
                        after.pointer("/mcp/other").is_some(),
                        "opencode sibling missing"
                    );
                    assert!(
                        after.pointer("/mcp/rustcodegraph").is_some(),
                        "opencode rustcodegraph missing"
                    );
                } else {
                    assert!(
                        after.pointer("/mcpServers/other").is_some(),
                        "sibling missing"
                    );
                    assert!(
                        after.pointer("/mcpServers/rustcodegraph").is_some(),
                        "rustcodegraph missing"
                    );
                }
                target.uninstall(location);
            }
        }
    });
}

#[test]
fn uninstall_reverses_install_already_configured_returns_to_false() {
    with_fixture("contract-uninstall", |_| {
        for target in all_targets() {
            for location in supported_locations(target.as_ref()) {
                target.install(location, install_options(true));
                assert!(target.detect(location).already_configured);
                target.uninstall(location);
                assert!(!target.detect(location).already_configured);
            }
        }
    });
}

#[test]
fn print_config_returns_non_empty_output_without_writing_anything() {
    with_fixture("contract-print", |fixture| {
        for target in all_targets() {
            for location in supported_locations(target.as_ref()) {
                let mut before = list_all_files(fixture.home());
                before.extend(list_all_files(fixture.cwd()));
                before.sort();
                let out = target.print_config(location);
                assert!(
                    !out.is_empty(),
                    "{} printConfig returned empty",
                    target.id().as_str()
                );
                let mut after = list_all_files(fixture.home());
                after.extend(list_all_files(fixture.cwd()));
                after.sort();
                assert_eq!(
                    after,
                    before,
                    "{} printConfig wrote files",
                    target.id().as_str()
                );
            }
        }
    });
}
