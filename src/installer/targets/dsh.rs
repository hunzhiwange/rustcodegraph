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
// Edit only block-list insert rows whose own plugin name identifies the MCP client.
// Keep unsupported YAML untouched rather than removing a containing user group.
fn remove_patch(p: &std::path::Path) -> FileWrite {
    let before = read_text(p);
    let eol = if before.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines: Vec<String> = before.lines().map(str::to_owned).collect();
    fn trivia(s: &str) -> bool {
        s.trim().is_empty() || s.trim() == "---" || s.trim_start().starts_with('#')
    }
    fn end(lines: &[String], start: usize, mut stop: usize) -> usize {
        while stop > start + 1 && trivia(&lines[stop - 1]) {
            stop -= 1;
        }
        stop
    }
    fn indent(s: &str) -> usize {
        s.bytes().take_while(|b| *b == b' ').count()
    }
    let top = regex::Regex::new(r"^-(\s|$)").unwrap();
    let insert = regex::Regex::new(r"^-\s+insert\s*:\s*(#.*)?$").unwrap();
    let row_start = regex::Regex::new(r"^ +-(\s|$)").unwrap();
    let client = regex::Regex::new(
        r#"^\s*(?:-\s+)?["']?name["']?\s*:\s*["']?@deepseek-ai/dsh-mcp-client["']?\s*(?:#.*)?$"#,
    )
    .unwrap();
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| top.is_match(l).then_some(i))
        .collect();
    let mut changed = false;
    let items: Vec<_> = starts
        .iter()
        .enumerate()
        .map(|(n, &start)| {
            (
                start,
                end(
                    &lines,
                    start,
                    starts.get(n + 1).copied().unwrap_or(lines.len()),
                ),
            )
        })
        .collect();
    for (start, stop) in items.into_iter().rev() {
        if !insert.is_match(&lines[start]) {
            continue;
        }
        let Some(first) = (start + 1..stop).find(|&i| !trivia(&lines[i])) else {
            continue;
        };
        if !row_start.is_match(&lines[first]) {
            continue;
        }
        let level = indent(&lines[first]);
        let mut rows = Vec::new();
        let mut limit = stop;
        for (i, line) in lines.iter().enumerate().take(stop).skip(first) {
            if trivia(line) {
                continue;
            }
            if indent(line) < level {
                limit = i;
                break;
            }
            if indent(line) == level {
                rows.push(i);
            }
        }
        let ranges: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(i, &a)| (a, end(&lines, a, rows.get(i + 1).copied().unwrap_or(limit))))
            .collect();
        let ours: Vec<_> = ranges
            .iter()
            .copied()
            .filter(|&(a, b)| {
                let key_level = indent(&lines[a].replacen('-', " ", 1));
                lines[a..b].iter().any(|l| has_server(l))
                    && lines[a..b]
                        .iter()
                        .enumerate()
                        .any(|(i, l)| (i == 0 || indent(l) == key_level) && client.is_match(l))
            })
            .collect();
        if ours.is_empty() {
            continue;
        }
        if ours.len() == ranges.len() {
            if ours.last().unwrap().1 != stop {
                continue;
            }
            let begin = if start > 0 && lines[start - 1].trim().is_empty() {
                start - 1
            } else {
                start
            };
            lines.drain(begin..stop);
        } else {
            for (a, b) in ours.into_iter().rev() {
                lines.drain(a..b);
            }
        }
        changed = true;
    }
    if !changed {
        return file_write(path_to_string(p), WriteAction::NotFound);
    }
    if lines.iter().all(|l| trivia(l)) {
        lines.push("[]".into());
    }
    let mut after = lines.join(eol);
    if !after.ends_with(eol) {
        after.push_str(eol);
    }
    atomic_write_file_sync(p, &after).expect("remove DSH patch");
    file_write(path_to_string(p), WriteAction::Removed)
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
        let mut result = WriteResult::with_file(remove_patch(&path()));
        for profile in profiles() {
            let file = remove_patch(&profile);
            if file.action == WriteAction::Removed {
                result.files.push(file);
            }
        }
        result
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
