//! Gin middleware-chain synthesis.
//!
//! Gin 把 middleware/handler 注册到 engine/router 的 handlers 切片里，真实调度点在
//! 框架内部遍历 handlers。这里把框架 dispatcher 连到用户注册的 handler。

use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde_json::json;

use crate::db::queries::QueryBuilder;
use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};
use crate::resolution::types::ResolutionContext;
use crate::types::{Edge, EdgeKind, Language, Node, NodeKind};

use super::common::{MAX_CALLBACKS_PER_CHANNEL, edge, slice_lines};

pub(super) fn gin_middleware_chain_edges(
    queries: &mut QueryBuilder,
    ctx: &mut dyn ResolutionContext,
) -> Vec<Edge> {
    // 先找框架内部会调用 `.handlers[...]` 的 dispatcher，再全仓扫描 Go 注册调用。
    // dispatcher 数量很少，handler fanout 用公共 cap 控制。
    static DISPATCH_RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r#"\.handlers\s*\[[^\]]*\]\s*\("#).unwrap());
    static REG_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"\.(?:Use|GET|POST|PUT|PATCH|DELETE|OPTIONS|HEAD|Any|Handle)\s*\("#).unwrap()
    });
    let mut dispatchers = Vec::new();
    for method in queries
        .get_nodes_by_kind(NodeKind::Method)
        .unwrap_or_default()
    {
        if method.language != Language::Go {
            continue;
        }
        if !gin_dispatcher_name_has_receiver(&method.name) {
            continue;
        }
        let src = ctx
            .read_file(&method.file_path)
            .and_then(|c| slice_lines(&c, method.start_line, method.end_line));
        if src
            .as_deref()
            .map(|s| DISPATCH_RE.is_match(s))
            .unwrap_or(false)
        {
            dispatchers.push(method);
        }
    }
    if dispatchers.is_empty() {
        return Vec::new();
    }
    let dispatchers = dedupe_gin_dispatchers(dispatchers);
    let mut registered: HashMap<String, String> = HashMap::new();
    for file in ctx
        .get_all_files()
        .into_iter()
        .filter(|f| f.ends_with(".go"))
    {
        let Some(content) = ctx.read_file(&file) else {
            continue;
        };
        if !content.contains(".Use(") && !REG_RE.is_match(&content) {
            continue;
        }
        // 注释中的示例路由不能参与注册点匹配。
        let safe = strip_comments_for_regex(&content, CommentLang::Go);
        for mat in REG_RE.find_iter(&safe) {
            let paren_idx = mat.end().saturating_sub(1);
            let Some(args) = balanced_args(&safe, paren_idx) else {
                continue;
            };
            let line = safe[..mat.start()].lines().count() as u64 + 1;
            for arg in split_args(&args) {
                if let Some(name) = handler_ident(&arg) {
                    registered
                        .entry(name)
                        .or_insert_with(|| format!("{file}:{line}"));
                }
            }
        }
    }
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for dispatcher in dispatchers {
        let mut added = 0;
        for (name, registered_at) in &registered {
            if added >= MAX_CALLBACKS_PER_CHANNEL {
                break;
            }
            let handler = ctx.get_nodes_by_name(name).into_iter().find(|node| {
                matches!(node.kind, NodeKind::Function | NodeKind::Method)
                    && node.language == Language::Go
            });
            let Some(handler) = handler else {
                continue;
            };
            if handler.id == dispatcher.id {
                continue;
            }
            let key = format!("{}>{}", dispatcher.id, handler.id);
            if seen.insert(key) {
                edges.push(edge(
                    &dispatcher.id,
                    &handler.id,
                    EdgeKind::Calls,
                    Some(dispatcher.start_line),
                    "gin-middleware-chain",
                    [("via", json!(name)), ("registeredAt", json!(registered_at))],
                ));
                added += 1;
            }
        }
    }
    edges
}

fn dedupe_gin_dispatchers(dispatchers: Vec<Node>) -> Vec<Node> {
    // gin 的抽取可能同时给出短名/带 receiver 的同一 dispatcher；保留更具体的
    // receiver 版本，避免注册 handler 被重复连接。
    let mut deduped: Vec<Node> = Vec::new();
    for dispatcher in dispatchers {
        let duplicate = deduped.iter().position(|existing| {
            existing.file_path == dispatcher.file_path
                && gin_dispatcher_short_name(&existing.name)
                    == gin_dispatcher_short_name(&dispatcher.name)
                && gin_dispatcher_ranges_match(existing, &dispatcher)
        });
        if let Some(index) = duplicate {
            if gin_dispatcher_rank(&dispatcher) < gin_dispatcher_rank(&deduped[index]) {
                deduped[index] = dispatcher;
            }
        } else {
            deduped.push(dispatcher);
        }
    }
    deduped.sort_by_key(|node| {
        (
            node.file_path.clone(),
            node.start_line,
            node.end_line,
            node.name.clone(),
        )
    });
    deduped
}

fn gin_dispatcher_short_name(name: &str) -> &str {
    name.rsplit(['.', ':'])
        .find(|part| !part.is_empty())
        .unwrap_or(name)
}

fn gin_dispatcher_ranges_match(left: &Node, right: &Node) -> bool {
    let overlaps = left.start_line <= right.end_line && right.start_line <= left.end_line;
    let nearby_start = left.start_line.abs_diff(right.start_line) <= 2;
    overlaps || nearby_start
}

fn gin_dispatcher_rank(node: &Node) -> (u8, usize) {
    let qualified_name_in_name = gin_dispatcher_name_has_receiver(&node.name);
    (u8::from(!qualified_name_in_name), node.id.len())
}

fn gin_dispatcher_name_has_receiver(name: &str) -> bool {
    name.contains('.') || name.contains("::")
}

fn balanced_args(src: &str, open_idx: usize) -> Option<String> {
    // 路由注册参数里可能有嵌套调用或数组，不能简单 split 到第一个 `)`。
    let mut depth = 0i32;
    for (idx, ch) in src.char_indices().skip_while(|(idx, _)| *idx < open_idx) {
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(src[open_idx + 1..idx].to_string());
            }
        }
    }
    None
}

fn split_args(args: &str) -> Vec<String> {
    // 只按顶层逗号拆参数，保留 gin.Group(...), []HandlerFunc{...} 这类嵌套表达式。
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in args.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                cur.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn handler_ident(expr: &str) -> Option<String> {
    // 匿名函数没有稳定目标节点；这里只保留可按名字回查的 handler。
    let cleaned = expr.trim().trim_end_matches("()").trim();
    if cleaned.is_empty()
        || cleaned.starts_with('"')
        || cleaned.starts_with('`')
        || cleaned.starts_with("func")
    {
        return None;
    }
    let ident = cleaned
        .rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .find(|s| !s.is_empty())?;
    Some(ident.to_string())
}
