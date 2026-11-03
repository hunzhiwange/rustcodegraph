//! DeepSeek Harness home-level Cordis patches; never writes instructions.
use super::{shared::*, types::*};
use std::path::PathBuf;
#[derive(Debug, Clone, Copy, Default)]
pub struct DshTarget;
fn home() -> PathBuf {
    config_dir_from_env("DSH_HOME", home_dir().join(".dsh"))
}
fn path() -> PathBuf {
    home().join("cordis.patch.yml")
}
fn profiles() -> Vec<PathBuf> {
    std::fs::read_dir(home().join("profiles"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path().join("cordis.patch.yml"))
        .filter(|p| p.is_file())
        .collect()
}
fn has_server(text: &str) -> bool {
    let re = regex::Regex::new(
        r#"(^|[\s{,])["']?serverName["']?\s*:\s*["']?rustcodegraph["']?\s*([,}#]|$)"#,
    )
    .unwrap();
    text.lines()
        .any(|l| !l.trim_start().starts_with('#') && re.is_match(l))
}
fn entry() -> String {
    let c = get_mcp_server_config();
    format!(
        "- insert:\n    - id: mcp-rustcodegraph\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        serverName: rustcodegraph\n        transport: stdio\n        command: {}\n        args:\n{}\n",
        c.command,
        c.args
            .iter()
            .map(|a| format!("          - {a}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}
fn refused() -> WriteResult {
    WriteResult::empty().with_notes(["DeepSeek Harness uses $DSH_HOME; use --location=global."])
}
impl AgentTarget for DshTarget {
    fn id(&self) -> TargetId {
        TargetId::Dsh
    }
    fn display_name(&self) -> &'static str {
        "DeepSeek Harness"
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some(
            "https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/mcp/mcp-client/README.md",
        )
    }
    fn supports_location(&self, l: Location) -> bool {
        l == Location::Global
    }
    fn detect(&self, l: Location) -> DetectionResult {
        if !self.supports_location(l) {
            return DetectionResult::default();
        }
        DetectionResult {
            installed: home().exists(),
            already_configured: has_server(&read_text(path()))
                || profiles().iter().any(|p| has_server(&read_text(p))),
            config_path: Some(path_to_string(path())),
        }
    }
    fn install(&self, l: Location, _: InstallOptions) -> WriteResult {
        if !self.supports_location(l) {
            return refused();
        }
        let p = path();
        let before = read_text(&p);
        if has_server(&before) || profiles().iter().any(|p| has_server(&read_text(p))) {
            return WriteResult::with_file(file_write(path_to_string(p), WriteAction::Unchanged));
        }
        let content: Vec<_> = before
            .lines()
            .filter(|l| {
                !l.trim().is_empty() && !l.trim_start().starts_with('#') && l.trim() != "---"
            })
            .collect();
        let empty =
            content.len() == 1 && content[0].trim().split('#').next().unwrap().trim() == "[]";
        let list = content.first().is_some_and(|l| l.starts_with("- "))
            && content
                .iter()
                .all(|l| l.starts_with("- ") || l.starts_with(' ') || l.starts_with('\t'));
        if !content.is_empty() && !empty && !list {
            return WriteResult::empty().with_notes(["Patch is not a block-style YAML list; nothing changed. Add --print-config dsh manually."]);
        }
        let mut text = if empty {
            before
                .lines()
                .filter(|l| l.trim().split('#').next().unwrap().trim() != "[]")
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            before.clone()
        };
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        if list {
            text.push('\n');
        }
        text.push_str(&entry());
        if before.contains("\r\n") {
            text = text.replace("\r\n", "\n").replace('\n', "\r\n");
        }
        let action = if p.exists() {
            WriteAction::Updated
        } else {
            WriteAction::Created
        };
        atomic_write_file_sync(&p, &text).expect("write DSH patch");
        WriteResult::with_file(file_write(path_to_string(p), action))
    }
    fn uninstall(&self, l: Location) -> WriteResult {
        if !self.supports_location(l) {
            return refused();
        }
        let p = path();
        let before = read_text(&p);
        // Only remove the exact standalone patch emitted by this installer.
        // Custom and profile rows require the separate roundtrip implementation.
        let managed = entry();
        if before.contains(&managed) {
            let mut after = before.replacen(&managed, "", 1);
            if after
                .lines()
                .all(|l| l.trim().is_empty() || l.trim_start().starts_with('#'))
            {
                after.push_str("[]\n");
            }
            atomic_write_file_sync(&p, &after).expect("remove DSH patch");
            return WriteResult::with_file(file_write(path_to_string(p), WriteAction::Removed));
        }
        WriteResult::with_file(file_write(path_to_string(p), WriteAction::NotFound))
    }
    fn print_config(&self, l: Location) -> String {
        if !self.supports_location(l) {
            return "# Use --location=global.\n".into();
        }
        format!("# Add to {}\n\n{}", path_to_string(path()), entry())
    }
    fn describe_paths(&self, l: Location) -> Vec<String> {
        if self.supports_location(l) {
            vec![path_to_string(path())]
        } else {
            vec![]
        }
    }
}
