use super::common::*;
use rustcodegraph::installer::config_writer::write_mcp_config;

#[test]
fn jsonc_server_strings_roundtrip_without_corrupting_escapes() {
    use rustcodegraph::installer::config_writer::upsert_mcp_server_under;

    with_fixture("jsonc-escapes", |f| {
        let entry = json!({
            "command": r#"C:\Program Files\agent\"quoted"\run.exe"#,
            "args": [r"C:\Users\runneradmin\repo", r"\\server\share\repo", "line\n\r\t\u{0000}\u{0008}\u{000c}\u{001f}", "中文"],
            "env": {"CUSTOM\\KEY\n\"": "value\\\"\n"},
            "enabled": true,
            "timeout": 42,
            "optional": null
        });
        for wrapper in ["mcpServers", "servers"] {
            for replace in [false, true] {
                let file = f.cwd().join(format!("{wrapper}-{replace}.json"));
                let before = format!(
                    "{{\r\n  // keep this\r\n  \"{wrapper}\": {{\"other\": {{\"command\": \"keep\"}}}}\r\n}}\r\n"
                );
                fs::write(&file, before).unwrap();
                assert_eq!(
                    upsert_mcp_server_under(&file, wrapper, &entry, replace)
                        .unwrap()
                        .action,
                    WriteAction::Updated
                );
                let installed = read_text(&file);
                let parsed = jsonc_parser::parse_to_serde_value(&installed, &Default::default())
                    .unwrap()
                    .unwrap();
                assert_eq!(parsed[wrapper]["rustcodegraph"], entry);
                assert_eq!(parsed[wrapper]["other"]["command"], "keep");
                assert!(installed.contains("// keep this\r\n"));
                assert!(!installed.replace("\r\n", "").contains('\n'));
                assert_eq!(
                    upsert_mcp_server_under(&file, wrapper, &entry, replace)
                        .unwrap()
                        .action,
                    WriteAction::Unchanged
                );
                assert_eq!(read_text(&file), installed);
            }
        }
    });
}

#[test]
fn case_033_preserves_comments_and_user_keys() {
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let env = Fixture::new("033-comments");
    let file = env.cwd().join(".mcp.json");
    fs::write(&file, "{\n    // keep this\n    \"custom\": 42,\n    \"mcpServers\": {\"rustcodegraph\": {\"command\": \"pinned\", \"disabled\": true}}\n}\n").unwrap();
    write_mcp_config(Location::Local);
    let after = fs::read_to_string(&file).unwrap();
    assert!(after.contains("    // keep this"));
    assert!(after.contains("\"command\": \"pinned\""));
    assert!(after.contains("\"disabled\": true"));
    assert!(after.contains("\"args\""));
    assert_eq!(
        write_mcp_config(Location::Local).action,
        WriteAction::Unchanged
    );
    assert_eq!(fs::read_to_string(file).unwrap(), after);
}

#[test]
fn case_033_rejects_malformed_without_changes() {
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let env = Fixture::new("033-malformed");
    let file = env.cwd().join(".mcp.json");
    let before = "{ broken !!!";
    fs::write(&file, before).unwrap();
    assert!(std::panic::catch_unwind(|| write_mcp_config(Location::Local)).is_err());
    assert_eq!(fs::read_to_string(file).unwrap(), before);
}

#[test]
fn case_033_rejects_nonobject_servers_without_changes() {
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let env = Fixture::new("033-shape");
    let file = env.cwd().join(".mcp.json");
    let before = "{\"mcpServers\": []}";
    fs::write(&file, before).unwrap();
    assert!(std::panic::catch_unwind(|| write_mcp_config(Location::Local)).is_err());
    assert_eq!(fs::read_to_string(file).unwrap(), before);
}

#[test]
fn case_033_fresh_install_and_trailing_comma() {
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let env = Fixture::new("033-fresh");
    assert_eq!(
        write_mcp_config(Location::Local).action,
        WriteAction::Created
    );
    let file = env.cwd().join(".mcp.json");
    fs::write(&file, "{\n    // sibling\n    \"custom\": 42,\n}\n").unwrap();
    write_mcp_config(Location::Local);
    let after = fs::read_to_string(&file).unwrap();
    assert!(after.contains("    // sibling\n    \"custom\": 42,"));
    assert!(after.contains("    \"mcpServers\""));
    assert_eq!(
        write_mcp_config(Location::Local).action,
        WriteAction::Unchanged
    );
}
