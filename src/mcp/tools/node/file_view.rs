//! File-view path for `rustcodegraph_node`.
//!
//! 当用户传 `file` 而不是 `symbol` 时，这里返回类似 Read 的带行号源码，
//! 同时附上 dependents；配置文件只展示键名以避免泄露值。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::super::DEFAULT_NODE_FILE_VIEW_LINES;
use super::super::shared::{node_kind_label, normalize_slashes};
use crate::types::{FileRecord, Language, NodeKind};

pub(super) fn node_file_view_result(
    cg: &mut crate::CodeGraph,
    project_root: &Path,
    file_hint: &str,
    symbols_only: bool,
    offset: usize,
    limit: Option<usize>,
) -> String {
    let Some(file) = find_indexed_file(cg, file_hint) else {
        return format!("No indexed file matches `{file_hint}`.");
    };

    if symbols_only {
        return format_file_symbols(cg, &file);
    }

    if is_config_file(&file) {
        return format_config_file_summary(project_root, &file);
    }

    let source_path = project_file_path(project_root, &file.path);
    let Ok(source) = fs::read_to_string(&source_path) else {
        return format!("Indexed file `{}` could not be read from disk.", file.path);
    };
    let dependents = file_dependents(cg, project_root, &file);
    format_file_source_view(&file, &source, &dependents, offset, limit)
}

fn find_indexed_file(cg: &mut crate::CodeGraph, file_hint: &str) -> Option<FileRecord> {
    // 匹配优先级从完整相对路径到 basename，保证 `src/foo.rs` 胜过任意同名文件。
    let hint = normalize_file_lookup_path(file_hint);
    if hint.is_empty() {
        return None;
    }

    let mut matches = cg
        .get_files()
        .into_iter()
        .filter_map(|file| file_match_rank(&file.path, &hint).map(|rank| (rank, file)))
        .collect::<Vec<_>>();
    matches.sort_by(|(rank_a, file_a), (rank_b, file_b)| {
        rank_a
            .cmp(rank_b)
            .then_with(|| file_a.path.len().cmp(&file_b.path.len()))
            .then_with(|| file_a.path.cmp(&file_b.path))
    });
    matches.into_iter().map(|(_, file)| file).next()
}

fn file_match_rank(path: &str, hint: &str) -> Option<usize> {
    let normalized_path = normalize_file_lookup_path(path);
    if normalized_path == hint {
        return Some(0);
    }
    if normalized_path.ends_with(&format!("/{hint}")) {
        return Some(1);
    }
    if hint.ends_with(&format!("/{normalized_path}")) {
        return Some(2);
    }
    if normalized_path.rsplit('/').next() == Some(hint) {
        return Some(3);
    }
    None
}

fn normalize_file_lookup_path(path: &str) -> String {
    let mut normalized = normalize_slashes(path);
    loop {
        if let Some(rest) = normalized.strip_prefix("./") {
            normalized = rest.to_string();
            continue;
        }
        if normalized.starts_with('/') {
            normalized = normalized.trim_start_matches('/').to_string();
            continue;
        }
        break;
    }
    while normalized.ends_with('/') {
        normalized.pop();
    }
    normalized
}

fn project_file_path(project_root: &Path, file_path: &str) -> PathBuf {
    let path = Path::new(file_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}

fn is_config_file(file: &FileRecord) -> bool {
    matches!(file.language, Language::Yaml | Language::Properties)
}

fn format_config_file_summary(project_root: &Path, file: &FileRecord) -> String {
    let source =
        fs::read_to_string(project_file_path(project_root, &file.path)).unwrap_or_default();
    let keys = config_keys(&source);
    let mut lines = vec![
        format!("## {}", file.path),
        "Config/data file values withheld.".to_string(),
    ];
    if !keys.is_empty() {
        lines.push(String::new());
        lines.push("### Keys".to_string());
        for key in keys {
            lines.push(format!("- `{key}`"));
        }
    }
    lines.join("\n")
}

fn config_keys(source: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut seen = HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        let key = trimmed
            .split_once('=')
            .map(|(key, _)| key)
            .or_else(|| trimmed.split_once(':').map(|(key, _)| key))
            .unwrap_or(trimmed)
            .trim();
        if key.is_empty() || key.starts_with('-') {
            continue;
        }
        if seen.insert(key.to_string()) {
            keys.push(key.to_string());
        }
    }
    keys
}

fn format_file_symbols(cg: &mut crate::CodeGraph, file: &FileRecord) -> String {
    let mut nodes = cg.get_nodes_in_file(&file.path);
    nodes.retain(|node| {
        !matches!(
            node.kind,
            NodeKind::File | NodeKind::Import | NodeKind::Export | NodeKind::Parameter
        )
    });
    nodes.sort_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then_with(|| a.name.cmp(&b.name))
    });

    let mut lines = vec![
        format!("## {}", file.path),
        String::new(),
        "### Symbols".to_string(),
    ];
    if nodes.is_empty() {
        lines.push("No symbols indexed for this file.".to_string());
    } else {
        for node in nodes {
            let display_name = if node.qualified_name.is_empty() {
                node.name.as_str()
            } else {
                node.qualified_name.as_str()
            };
            lines.push(format!(
                "- {} `{}` at line {}",
                node_kind_label(node.kind),
                display_name,
                node.start_line
            ));
        }
    }
    lines.join("\n")
}

fn file_dependents(
    cg: &mut crate::CodeGraph,
    project_root: &Path,
    file: &FileRecord,
) -> Vec<String> {
    // DB 已有 imports/edges 时直接用；没有时用文本 import 字符串做最后一层提示，
    // 让 file view 仍能给出基本 blast radius。
    let mut dependents = cg.get_file_dependents(&file.path);
    if dependents.is_empty() {
        dependents = find_textual_file_dependents(cg, project_root, file);
    }
    let target_path = normalize_file_lookup_path(&file.path);
    dependents.retain(|dependent| normalize_file_lookup_path(dependent) != target_path);
    dependents.sort();
    dependents.dedup();
    dependents
}

fn find_textual_file_dependents(
    cg: &mut crate::CodeGraph,
    project_root: &Path,
    file: &FileRecord,
) -> Vec<String> {
    cg.get_files()
        .into_iter()
        .filter(|candidate| candidate.path != file.path)
        .filter_map(|candidate| {
            let source =
                fs::read_to_string(project_file_path(project_root, &candidate.path)).ok()?;
            let modules = import_module_candidates(&candidate.path, &file.path);
            if modules
                .iter()
                .any(|module| source_contains_module(&source, module))
            {
                Some(candidate.path)
            } else {
                None
            }
        })
        .collect()
}

fn import_module_candidates(importer_path: &str, target_path: &str) -> Vec<String> {
    let target_no_ext = strip_last_extension(&normalize_file_lookup_path(target_path));
    let relative_no_ext = relative_module_path(importer_path, &target_no_ext);
    let mut modules = vec![relative_no_ext.clone(), target_no_ext.clone()];
    if relative_no_ext != target_no_ext {
        modules.push(
            target_no_ext
                .rsplit('/')
                .next()
                .unwrap_or(&target_no_ext)
                .to_string(),
        );
    }
    modules.sort();
    modules.dedup();
    modules
}

fn strip_last_extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| path.to_string())
}

fn relative_module_path(importer_path: &str, target_no_ext: &str) -> String {
    let importer_dir = normalize_file_lookup_path(importer_path)
        .rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default();
    let importer_parts = importer_dir
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let target_parts = target_no_ext
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let common = importer_parts
        .iter()
        .zip(target_parts.iter())
        .take_while(|(left, right)| left == right)
        .count();

    let mut relative = Vec::new();
    relative.extend(std::iter::repeat_n(
        "..",
        importer_parts.len().saturating_sub(common),
    ));
    relative.extend(target_parts.iter().skip(common).copied());
    let joined = relative.join("/");
    if joined.starts_with('.') {
        joined
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        format!("./{joined}")
    }
}

fn source_contains_module(source: &str, module: &str) -> bool {
    source.contains(&format!("'{module}'")) || source.contains(&format!("\"{module}\""))
}

fn format_file_source_view(
    file: &FileRecord,
    source: &str,
    dependents: &[String],
    offset: usize,
    limit: Option<usize>,
) -> String {
    // offset/limit 与常见 Read 工具保持同样的一基行号语义，方便 agent 直接编辑。
    let mut lines = vec![
        format!("## {}", file.path),
        format_dependents_header(dependents),
    ];
    let source_lines = source.lines().collect::<Vec<_>>();
    let total = source_lines.len();
    if total == 0 {
        lines.push(String::new());
        lines.push("File is empty.".to_string());
        return lines.join("\n");
    }
    if offset > total {
        lines.push(String::new());
        lines.push(format!(
            "Offset {offset} is past the end of `{}` ({total} lines).",
            file.path
        ));
        return lines.join("\n");
    }

    let requested_limit = limit.unwrap_or(DEFAULT_NODE_FILE_VIEW_LINES);
    let end = (offset + requested_limit - 1).min(total);
    if offset != 1 || limit.is_some() || end < total {
        lines.push(format!("Showing lines {offset}-{end} of {total}."));
    }
    lines.push(String::new());
    lines.extend(numbered_lines(&source_lines, offset, end));
    lines.join("\n")
}

fn format_dependents_header(dependents: &[String]) -> String {
    if dependents.is_empty() {
        return "used by 0 files".to_string();
    }
    let noun = if dependents.len() == 1 {
        "file"
    } else {
        "files"
    };
    let shown = dependents.iter().take(5).cloned().collect::<Vec<_>>();
    let mut header = format!("used by {} {noun}: {}", dependents.len(), shown.join(", "));
    if dependents.len() > shown.len() {
        header.push_str(&format!(", and {} more", dependents.len() - shown.len()));
    }
    header
}

pub(super) fn numbered_lines(source_lines: &[&str], start: usize, end: usize) -> Vec<String> {
    source_lines[(start - 1)..end]
        .iter()
        .enumerate()
        .map(|(index, line)| format!("{}\t{}", start + index, line))
        .collect()
}
