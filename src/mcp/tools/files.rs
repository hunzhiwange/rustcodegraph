//! File-listing MCP tool implementation.
//!
//! `rustcodegraph_files` 面向项目结构浏览，输出 flat/grouped/tree 三种格式；
//! 它只使用索引中的文件表，不读取源码内容。

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde_json::Value;

use super::ToolHandler;
use super::response::{ToolResult, error_result, text_result, truncate_output};
use super::shared::{glob_to_regex, language_label};
use super::validation::{not_indexed_message, unindexed_project_path_message};
use crate::types::{FileRecord, Language};

pub(super) fn run(
    handler: &mut ToolHandler,
    args: &serde_json::Map<String, Value>,
    project_path: Option<&str>,
    project_root: Option<&str>,
) -> ToolResult {
    let Some(project_root) = project_root else {
        return text_result(&not_indexed_message(
            handler.default_project_hint.as_deref(),
        ));
    };
    let Ok(mut cg) = crate::CodeGraph::open_sync(Path::new(project_root)) else {
        return text_result(&unindexed_project_path_message(project_root));
    };

    let path_filter = args.get("path").and_then(Value::as_str);
    let pattern = args.get("pattern").and_then(Value::as_str);
    let format = args.get("format").and_then(Value::as_str).unwrap_or("tree");
    let include_metadata = args
        .get("includeMetadata")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let max_depth = args
        .get("maxDepth")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, 20) as usize);

    let all_files = cg.get_files();
    cg.close();
    if all_files.is_empty() {
        return text_result("No files indexed. Run `rustcodegraph index` first.");
    }

    let normalized_filter = normalize_files_path_filter(path_filter);
    let mut files = if normalized_filter.is_empty() {
        all_files
    } else {
        let prefix = format!("{normalized_filter}/");
        all_files
            .into_iter()
            .filter(|file| file.path == normalized_filter || file.path.starts_with(&prefix))
            .collect::<Vec<_>>()
    };

    if let Some(pattern) = pattern {
        let Ok(regex) = glob_to_regex(pattern) else {
            return error_result("pattern must be a valid glob pattern");
        };
        files.retain(|file| regex.is_match(&file.path));
    }

    if files.is_empty() {
        return text_result("No files found matching the criteria.");
    }

    let output = match format {
        "flat" => format_files_flat(&files, include_metadata),
        "grouped" => format_files_grouped(&files, include_metadata),
        "tree" => format_files_tree(&files, include_metadata, max_depth),
        _ => format_files_tree(&files, include_metadata, max_depth),
    };
    let result = handler.with_index_state_notice(
        text_result(&truncate_output(&output)),
        project_root,
        &HashSet::new(),
    );
    handler.with_worktree_notice(result, project_path)
}

fn normalize_files_path_filter(path_filter: Option<&str>) -> String {
    // 用户可能传绝对样式、`./` 或 Windows 分隔符；索引内统一是项目相对 `/` 路径。
    let Some(path_filter) = path_filter else {
        return String::new();
    };
    if path_filter.is_empty() {
        return String::new();
    }

    let mut normalized = path_filter.replace('\\', "/");
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
    if normalized == "." {
        normalized.clear();
    }
    while normalized.ends_with('/') {
        normalized.pop();
    }
    normalized
}
fn format_files_flat(files: &[FileRecord], include_metadata: bool) -> String {
    let mut sorted = files.to_vec();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));

    let mut lines = vec![format!("## Files ({})", sorted.len()), String::new()];
    for file in sorted {
        if include_metadata {
            lines.push(format!(
                "- {} ({}, {} symbols)",
                file.path,
                language_label(file.language),
                file.node_count
            ));
        } else {
            lines.push(format!("- {}", file.path));
        }
    }
    lines.join("\n")
}
fn format_files_grouped(files: &[FileRecord], include_metadata: bool) -> String {
    let mut by_language: BTreeMap<String, Vec<FileRecord>> = BTreeMap::new();
    for file in files {
        by_language
            .entry(language_label(file.language))
            .or_default()
            .push(file.clone());
    }

    let mut groups = by_language.into_iter().collect::<Vec<_>>();
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));

    let mut lines = vec![
        format!("## Files by Language ({} total)", files.len()),
        String::new(),
    ];
    for (language, mut language_files) in groups {
        language_files.sort_by(|a, b| a.path.cmp(&b.path));
        lines.push(format!("### {} ({})", language, language_files.len()));
        for file in language_files {
            if include_metadata {
                lines.push(format!("- {} ({} symbols)", file.path, file.node_count));
            } else {
                lines.push(format!("- {}", file.path));
            }
        }
        lines.push(String::new());
    }
    lines.join("\n")
}
#[derive(Default)]
struct FileTreeNode {
    name: String,
    children: BTreeMap<String, FileTreeNode>,
    file: Option<(Language, u64)>,
}

fn format_files_tree(
    files: &[FileRecord],
    include_metadata: bool,
    max_depth: Option<usize>,
) -> String {
    let mut root = FileTreeNode::default();
    for file in files {
        let mut current = &mut root;
        let parts = file
            .path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        for (index, part) in parts.iter().enumerate() {
            current = current
                .children
                .entry((*part).to_string())
                .or_insert_with(|| FileTreeNode {
                    name: (*part).to_string(),
                    ..FileTreeNode::default()
                });
            if index == parts.len() - 1 {
                current.file = Some((file.language, file.node_count));
            }
        }
    }

    let mut lines = vec![
        format!("## Project Structure ({} files)", files.len()),
        String::new(),
    ];
    render_file_tree_node(&root, "", true, 0, max_depth, include_metadata, &mut lines);
    lines.join("\n")
}
fn render_file_tree_node(
    node: &FileTreeNode,
    prefix: &str,
    is_last: bool,
    depth: usize,
    max_depth: Option<usize>,
    include_metadata: bool,
    lines: &mut Vec<String>,
) {
    // maxDepth 是展示深度，不影响查询结果；超过深度直接截断子树。
    if max_depth.is_some_and(|max_depth| depth > max_depth) {
        return;
    }

    let connector = if is_last { "└── " } else { "├── " };
    let child_prefix = if is_last { "    " } else { "│   " };

    if !node.name.is_empty() {
        let mut line = format!("{prefix}{connector}{}", node.name);
        if let Some((language, node_count)) = node.file
            && include_metadata
        {
            line.push_str(&format!(
                " ({}, {} symbols)",
                language_label(language),
                node_count
            ));
        }
        lines.push(line);
    }

    let mut children = node.children.values().collect::<Vec<_>>();
    children.sort_by(|a, b| {
        let a_is_dir = !a.children.is_empty() && a.file.is_none();
        let b_is_dir = !b.children.is_empty() && b.file.is_none();
        b_is_dir.cmp(&a_is_dir).then_with(|| a.name.cmp(&b.name))
    });

    for (index, child) in children.iter().enumerate() {
        let last = index == children.len() - 1;
        let next_prefix = if node.name.is_empty() {
            prefix.to_string()
        } else {
            format!("{prefix}{child_prefix}")
        };
        render_file_tree_node(
            child,
            &next_prefix,
            last,
            depth + 1,
            max_depth,
            include_metadata,
            lines,
        );
    }
}
