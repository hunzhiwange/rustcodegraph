use super::common::*;

fn values(command: &str, args: &[&str]) -> BTreeMap<String, TomlValue> {
    let mut values = BTreeMap::new();
    values.insert("command".to_owned(), TomlValue::String(command.to_owned()));
    values.insert(
        "args".to_owned(),
        TomlValue::Strings(args.iter().map(|arg| (*arg).to_owned()).collect()),
    );
    values
}

#[test]
fn builds_mcp_servers_rustcodegraph_block_with_command_and_args() {
    let block = build_toml_table(
        "mcp_servers.rustcodegraph",
        values("rustcodegraph", &["serve", "--mcp"]),
    );
    assert!(block.contains("[mcp_servers.rustcodegraph]"));
    assert!(block.contains("command = \"rustcodegraph\""));
    assert!(block.contains("args = [\"serve\", \"--mcp\"]"));
}

#[test]
fn upsert_inserts_into_empty_content() {
    let block = build_toml_table(
        "mcp_servers.rustcodegraph",
        values("rustcodegraph", &["serve"]),
    );
    let result = upsert_toml_table("", "mcp_servers.rustcodegraph", &block);
    assert_eq!(result.action, UpsertTomlAction::Inserted);
    assert!(result.content.starts_with("[mcp_servers.rustcodegraph]"));
}

#[test]
fn upsert_is_idempotent_second_call_returns_unchanged() {
    let block = build_toml_table(
        "mcp_servers.rustcodegraph",
        values("rustcodegraph", &["serve"]),
    );
    let first = upsert_toml_table("", "mcp_servers.rustcodegraph", &block);
    let second = upsert_toml_table(&first.content, "mcp_servers.rustcodegraph", &block);
    assert_eq!(second.action, UpsertTomlAction::Unchanged);
    assert_eq!(second.content, first.content);
}

#[test]
fn upsert_replaces_existing_block_in_place_preserving_sibling_tables() {
    let existing = [
        "[other_table]",
        "foo = \"bar\"",
        "",
        "[mcp_servers.rustcodegraph]",
        "command = \"old-rustcodegraph\"",
        "args = [\"old\"]",
        "",
        "[zzz]",
        "baz = \"qux\"",
        "",
    ]
    .join("\n");
    let new_block = build_toml_table(
        "mcp_servers.rustcodegraph",
        values("rustcodegraph", &["serve", "--mcp"]),
    );
    let result = upsert_toml_table(&existing, "mcp_servers.rustcodegraph", &new_block);
    assert_eq!(result.action, UpsertTomlAction::Replaced);
    assert!(result.content.contains("[other_table]"));
    assert!(result.content.contains("foo = \"bar\""));
    assert!(result.content.contains("[zzz]"));
    assert!(result.content.contains("baz = \"qux\""));
    assert!(result.content.contains("command = \"rustcodegraph\""));
    assert!(!result.content.contains("old-rustcodegraph"));
}

#[test]
fn remove_toml_table_strips_block_and_preserves_siblings() {
    let existing = [
        "[other_table]",
        "foo = \"bar\"",
        "",
        "[mcp_servers.rustcodegraph]",
        "command = \"rustcodegraph\"",
        "args = [\"serve\"]",
    ]
    .join("\n");
    let result = remove_toml_table(&existing, "mcp_servers.rustcodegraph");
    assert_eq!(result.action, RemoveTomlAction::Removed);
    assert!(result.content.contains("[other_table]"));
    assert!(result.content.contains("foo = \"bar\""));
    assert!(!result.content.contains("mcp_servers.rustcodegraph"));
}

#[test]
fn remove_toml_table_on_missing_table_returns_not_found_no_content_change() {
    let existing = "[other]\nfoo = \"bar\"\n";
    let result = remove_toml_table(existing, "mcp_servers.rustcodegraph");
    assert_eq!(result.action, RemoveTomlAction::NotFound);
    assert_eq!(result.content, existing);
}

#[test]
fn upsert_preserves_array_of_tables_sibling() {
    let existing = ["[[foo]]", "name = \"a\"", "", "[[foo]]", "name = \"b\"", ""].join("\n");
    let block = build_toml_table(
        "mcp_servers.rustcodegraph",
        values("rustcodegraph", &["serve"]),
    );
    let result = upsert_toml_table(&existing, "mcp_servers.rustcodegraph", &block);
    assert_eq!(result.content.matches("[[foo]]").count(), 2);
    assert!(result.content.contains("[mcp_servers.rustcodegraph]"));
}
