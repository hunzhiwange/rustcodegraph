use super::common::*;

const MDC_FRONTMATTER: &str = "---\ndescription: CodeGraph MCP usage guide - when to use which tool\nalwaysApply: true\n---\n\n";

fn rules_file() -> PathBuf {
    env::current_dir()
        .unwrap()
        .join(".cursor")
        .join("rules")
        .join("codegraph.mdc")
}

fn plant_legacy_rules_file(extra: &str) {
    write_text(
        rules_file(),
        &(MDC_FRONTMATTER.to_owned() + LEGACY_BLOCK + "\n" + extra),
    );
}

#[test]
fn uninstall_leaves_leftover_codegraph_mdc_untouched() {
    with_fixture("cursor-rules-delete", |_| {
        let cursor = target("cursor");
        plant_legacy_rules_file("");
        assert!(rules_file().exists());
        cursor.uninstall(Location::Local);
        assert!(rules_file().exists());
        assert!(read_text(rules_file()).contains("CODEGRAPH_START"));
    });
}

#[test]
fn install_leaves_leftover_codegraph_mdc_untouched() {
    with_fixture("cursor-rules-install-heal", |_| {
        let cursor = target("cursor");
        plant_legacy_rules_file("");
        let result = cursor.install(Location::Local, install_options(true));
        assert!(rules_file().exists());
        assert!(
            !result
                .files
                .iter()
                .any(|file| file.path.ends_with("codegraph.mdc"))
        );
    });
}

#[test]
fn uninstall_preserves_codegraph_marked_block_and_user_content() {
    with_fixture("cursor-rules-user-content", |_| {
        let cursor = target("cursor");
        plant_legacy_rules_file("## My own rule\nkeep me\n");
        cursor.uninstall(Location::Local);
        assert!(rules_file().exists());
        let after = read_text(rules_file());
        assert!(after.contains("keep me"));
        assert!(after.contains("codegraph_search"));
        assert!(after.contains("CODEGRAPH_START"));
    });
}
