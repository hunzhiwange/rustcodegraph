//! Script-language require resolution.
//!
//! Lua/Luau 和 Ruby 的 require 多数只给字符串模块名。这里用文件后缀和当前位置
//! 邻近度选项目内文件节点，标准库/外部包保持未解析。

use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::NodeKind;

use super::common::{resolved, shared_char_prefix};

pub(super) fn resolve_lua_require(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    // Lua 模块名里的点通常映射成目录；同名文件多处存在时，选与当前文件路径
    // 公共前缀最长的候选。
    let name = reference.reference_name.as_str();
    if name.is_empty() {
        return None;
    }
    let base = if name.contains('.') {
        name.replace('.', "/")
    } else {
        name.to_string()
    };
    let suffixes = [
        format!("{base}.lua"),
        format!("{base}.luau"),
        format!("{base}/init.lua"),
        format!("{base}/init.luau"),
    ];
    for suffix in suffixes {
        let mut matches = context
            .get_all_files()
            .into_iter()
            .filter(|file| file == &suffix || file.ends_with(&format!("/{suffix}")))
            .collect::<Vec<_>>();
        if matches.is_empty() {
            continue;
        }
        matches
            .sort_by_key(|file| std::cmp::Reverse(shared_char_prefix(file, &reference.file_path)));
        let best = matches[0].clone();
        if best == reference.file_path {
            continue;
        }
        if let Some(file_node) = context
            .get_nodes_in_file(&best)
            .into_iter()
            .find(|n| n.kind == NodeKind::File)
        {
            return Some(resolved(reference, &file_node.id, 0.9, ResolvedBy::Import));
        }
    }
    None
}

pub(super) fn resolve_ruby_require(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    // Ruby 的 require_relative 只能从当前文件目录出发；普通 require 则允许
    // 在项目内任意同名路径中按邻近度选择。
    let name = reference.reference_name.trim();
    if name.is_empty() {
        return None;
    }
    let source_line = context
        .read_file(&reference.file_path)
        .and_then(|content| {
            content
                .lines()
                .nth(reference.line.saturating_sub(1) as usize)
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let require_relative = source_line.contains("require_relative");
    let base = name.trim_start_matches("./").trim_end_matches(".rb");
    let suffixes = [format!("{base}.rb"), format!("{base}/init.rb")];
    let from_dir = reference
        .file_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");

    for suffix in suffixes {
        let wanted = if require_relative && !from_dir.is_empty() {
            format!("{from_dir}/{suffix}")
        } else {
            suffix.clone()
        };
        let mut matches = context
            .get_all_files()
            .into_iter()
            .filter(|file| file == &wanted || file.ends_with(&format!("/{wanted}")))
            .collect::<Vec<_>>();
        if matches.is_empty() && !require_relative {
            matches = context
                .get_all_files()
                .into_iter()
                .filter(|file| file == &suffix || file.ends_with(&format!("/{suffix}")))
                .collect::<Vec<_>>();
        }
        if matches.is_empty() {
            continue;
        }
        matches
            .sort_by_key(|file| std::cmp::Reverse(shared_char_prefix(file, &reference.file_path)));
        let best = matches[0].clone();
        if best == reference.file_path {
            continue;
        }
        if let Some(file_node) = context
            .get_nodes_in_file(&best)
            .into_iter()
            .find(|node| node.kind == NodeKind::File)
        {
            return Some(resolved(reference, &file_node.id, 0.9, ResolvedBy::Import));
        }
    }
    None
}
