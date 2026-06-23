//! React render-path synthesis.
//!
//! React flow 需要同时跨过 `setState -> render` 和 `render -> child component`；
//! 只补其中一段会让 agent 继续钻源码，所以这里把两段都作为保守 heuristic 边。

use std::collections::HashSet;

use regex::Regex;
use serde_json::json;

use crate::db::queries::QueryBuilder;
use crate::resolution::types::ResolutionContext;
use crate::types::{Edge, EdgeKind, NodeKind};

use super::common::{
    MAX_CALLBACKS_PER_CHANNEL, MAX_JSX_CHILDREN, children_of_kind, edge, slice_lines,
};

pub(super) fn react_render_edges(
    queries: &mut QueryBuilder,
    ctx: &mut dyn ResolutionContext,
) -> Vec<Edge> {
    // class component 中 this.setState 会触发同类 render。函数组件 hook/dataflow
    // 目前不覆盖，避免把任意 setter 当成 render 触发器。
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for cls in queries
        .get_nodes_by_kind(NodeKind::Class)
        .unwrap_or_default()
    {
        let children = children_of_kind(queries, &cls.id, NodeKind::Method);
        let Some(render) = children.iter().find(|node| node.name == "render").cloned() else {
            continue;
        };
        let mut added = 0;
        for method in children {
            if added >= MAX_CALLBACKS_PER_CHANNEL || method.id == render.id {
                continue;
            }
            let src = ctx
                .read_file(&method.file_path)
                .and_then(|c| slice_lines(&c, method.start_line, method.end_line));
            if !src
                .as_deref()
                .map(|s| s.contains("this.setState("))
                .unwrap_or(false)
            {
                continue;
            }
            let key = format!("{}>{}", method.id, render.id);
            if !seen.insert(key) {
                continue;
            }
            edges.push(edge(
                &method.id,
                &render.id,
                EdgeKind::Calls,
                Some(method.start_line),
                "react-render",
                [
                    ("via", json!("setState")),
                    (
                        "registeredAt",
                        json!(format!("{}:{}", render.file_path, render.start_line)),
                    ),
                ],
            ));
            added += 1;
        }
    }
    edges
}

pub(super) fn react_jsx_child_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    // JSX 中大写 tag 约定为组件；小写 DOM tag 忽略。每个父节点限制 child 数量，
    // 防止大型 render 函数吞掉预算。
    static JSX_TAG_RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r#"<([A-Z][A-Za-z0-9_]*)[\s/>]"#).unwrap());
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for file in ctx.get_all_files() {
        let Some(content) = ctx.read_file(&file) else {
            continue;
        };
        if !content.contains("</") && !content.contains("/>") {
            continue;
        }
        let parents = ctx
            .get_nodes_in_file(&file)
            .into_iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    NodeKind::Method | NodeKind::Function | NodeKind::Component
                )
            })
            .collect::<Vec<_>>();
        for parent in parents {
            let Some(src) = slice_lines(&content, parent.start_line, parent.end_line) else {
                continue;
            };
            let mut names = HashSet::new();
            for cap in JSX_TAG_RE.captures_iter(&src) {
                names.insert(cap[1].to_string());
            }
            let mut added = 0;
            for name in names {
                if added >= MAX_JSX_CHILDREN {
                    break;
                }
                let child = ctx.get_nodes_by_name(&name).into_iter().find(|node| {
                    matches!(
                        node.kind,
                        NodeKind::Component | NodeKind::Function | NodeKind::Class
                    )
                });
                let Some(child) = child else {
                    continue;
                };
                if child.id == parent.id {
                    continue;
                }
                let key = format!("{}>{}", parent.id, child.id);
                if seen.insert(key) {
                    edges.push(edge(
                        &parent.id,
                        &child.id,
                        EdgeKind::Calls,
                        Some(parent.start_line),
                        "jsx-render",
                        [("via", json!(name))],
                    ));
                    added += 1;
                }
            }
        }
    }
    edges
}
