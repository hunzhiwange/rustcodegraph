//! General callback-channel synthesis for JS/TS-like code.
//!
//! 这些 pass 不依赖某个框架，而是识别常见“注册函数 -> dispatcher -> handler”
//! 模式。所有匹配都带 fanout 上限，宁可漏掉超大事件总线，也不写入高噪声全连接。

use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde_json::json;

use crate::db::queries::QueryBuilder;
use crate::resolution::types::ResolutionContext;
use crate::types::{Edge, EdgeKind, Node, NodeKind};

use super::common::{
    EVENT_FANOUT_CAP, MAX_CALLBACKS_PER_CHANNEL, edge, enclosing_fn, method_and_function_nodes,
    slice_lines,
};

fn registrar_name(name: &str) -> bool {
    (name.starts_with("on")
        && name
            .chars()
            .nth(2)
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false))
        || matches!(
            name,
            "subscribe"
                | "addListener"
                | "addEventListener"
                | "register"
                | "watch"
                | "listen"
                | "addCallback"
        )
}

fn dispatcher_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "emit", "trigger", "notify", "dispatch", "fire", "publish", "flush",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn registrar_field(src: &str) -> Option<String> {
    static RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r#"this\.(\w+)\.(?:add|push|set)\("#).unwrap());
    RE.captures(src)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

fn dispatcher_field(src: &str) -> Option<String> {
    static FOR_OF_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"\bof\s+(?:Array\.from\(\s*)?this\.(\w+)"#).unwrap()
    });
    static FOREACH_RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r#"this\.(\w+)\.forEach\("#).unwrap());
    if let Some(field) = FOR_OF_RE
        .captures(src)
        .and_then(|cap| cap.get(1))
        .filter(|_| Regex::new(r#"\b\w+\s*\("#).unwrap().is_match(src))
    {
        return Some(field.as_str().to_string());
    }
    FOREACH_RE
        .captures(src)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

pub(super) fn field_channel_edges(
    queries: &mut QueryBuilder,
    ctx: &mut dyn ResolutionContext,
) -> Vec<Edge> {
    // 识别 `onX(handler)` 把 handler 放进 `this.callbacks`，以及 `emitX()` 遍历
    // 同一字段的模式。限定同文件字段名，避免跨类同名字段误连。
    let mut registrars: Vec<(Node, String)> = Vec::new();
    let mut dispatchers: Vec<(Node, String)> = Vec::new();
    for node in method_and_function_nodes(queries) {
        let is_reg = registrar_name(&node.name);
        let is_disp = dispatcher_name(&node.name);
        if !is_reg && !is_disp {
            continue;
        }
        let Some(content) = ctx.read_file(&node.file_path) else {
            continue;
        };
        let Some(src) = slice_lines(&content, node.start_line, node.end_line) else {
            continue;
        };
        if is_reg && let Some(field) = registrar_field(&src) {
            registrars.push((node.clone(), field));
        }
        if is_disp && let Some(field) = dispatcher_field(&src) {
            dispatchers.push((node.clone(), field));
        }
    }

    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for (registrar, field) in registrars {
        let ch_dispatchers = dispatchers
            .iter()
            .filter(|(d, d_field)| d.file_path == registrar.file_path && d_field == &field)
            .cloned()
            .collect::<Vec<_>>();
        if ch_dispatchers.is_empty() {
            continue;
        }
        let arg_re = Regex::new(&format!(
            r#"{}\s*\(\s*(?:this\.)?(\w+)"#,
            regex::escape(&registrar.name)
        ))
        .unwrap();
        let mut added = 0;
        // 从 registrar 的 incoming calls 反查真实 handler 参数，避免只凭函数名
        // `onX`/`emitX` 把所有同字段方法都连起来。
        for incoming in queries
            .get_incoming_edges(&registrar.id, Some(vec![EdgeKind::Calls]))
            .unwrap_or_default()
        {
            if added >= MAX_CALLBACKS_PER_CHANNEL {
                break;
            }
            let Some(line) = incoming.line else {
                continue;
            };
            let Some(caller) = queries.get_node_by_id(&incoming.source).unwrap_or(None) else {
                continue;
            };
            let Some(content) = ctx.read_file(&caller.file_path) else {
                continue;
            };
            let Some(line_src) = content.lines().nth(line.saturating_sub(1) as usize) else {
                continue;
            };
            let Some(handler_name) = arg_re
                .captures(line_src)
                .and_then(|cap| cap.get(1))
                .map(|m| m.as_str())
            else {
                continue;
            };
            let Some(handler) = ctx
                .get_nodes_by_name(handler_name)
                .into_iter()
                .find(|node| matches!(node.kind, NodeKind::Method | NodeKind::Function))
            else {
                continue;
            };
            for (dispatcher, _) in &ch_dispatchers {
                if dispatcher.id == handler.id {
                    continue;
                }
                let key = format!("{}>{}", dispatcher.id, handler.id);
                if !seen.insert(key) {
                    continue;
                }
                edges.push(edge(
                    &dispatcher.id,
                    &handler.id,
                    EdgeKind::Calls,
                    Some(dispatcher.start_line),
                    "callback",
                    [
                        ("via", json!(registrar.name)),
                        ("field", json!(field)),
                        (
                            "registeredAt",
                            json!(format!("{}:{line}", caller.file_path)),
                        ),
                    ],
                ));
                added += 1;
            }
        }
    }
    edges
}

pub(super) fn closure_collection_edges(
    queries: &mut QueryBuilder,
    ctx: &mut dyn ResolutionContext,
) -> Vec<Edge> {
    // Kotlin/Swift/JS 等代码常把闭包 append 到集合，再 forEach 调用；这里用字段名
    // 作为 channel，但超过小规模 fanout 时放弃，防止全局 callback list 爆炸。
    static DISPATCH_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"(\w+)\.forEach\s*\{\s*(?:\$0|it)\s*\("#).unwrap()
    });
    static APPEND_WRITE_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"(\w+)\.write\s*\{\s*\$0(?:\.(\w+))?\.(?:append|add|push|insert)\s*\("#)
            .unwrap()
    });
    static APPEND_DIRECT_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"(\w+)\.(?:append|add|push|insert)\s*\("#).unwrap()
    });

    let mut dispatchers: HashMap<String, Vec<(Node, u64)>> = HashMap::new();
    let mut registrars: HashMap<String, Vec<(Node, u64)>> = HashMap::new();
    for node in method_and_function_nodes(queries) {
        let Some(content) = ctx.read_file(&node.file_path) else {
            continue;
        };
        let Some(src) = slice_lines(&content, node.start_line, node.end_line) else {
            continue;
        };
        if !src.contains(".forEach")
            && !src.contains(".append(")
            && !src.contains(".add(")
            && !src.contains(".push(")
            && !src.contains(".insert(")
        {
            continue;
        }
        let line_at = |idx: usize| node.start_line + src[..idx].lines().count() as u64;
        for cap in DISPATCH_RE.captures_iter(&src) {
            let field = cap[1].to_string();
            dispatchers
                .entry(field)
                .or_default()
                .push((node.clone(), line_at(cap.get(0).unwrap().start())));
        }
        for cap in APPEND_WRITE_RE.captures_iter(&src) {
            let field = cap
                .get(2)
                .or_else(|| cap.get(1))
                .unwrap()
                .as_str()
                .to_string();
            registrars
                .entry(field)
                .or_default()
                .push((node.clone(), line_at(cap.get(0).unwrap().start())));
        }
        for cap in APPEND_DIRECT_RE.captures_iter(&src) {
            let field = cap[1].to_string();
            if field.chars().all(|ch| ch.is_ascii_digit()) {
                continue;
            }
            registrars
                .entry(field)
                .or_default()
                .push((node.clone(), line_at(cap.get(0).unwrap().start())));
        }
    }

    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for (field, disps) in dispatchers {
        let Some(regs) = registrars.get(&field) else {
            continue;
        };
        if disps.len() > 8 || regs.len() > 8 {
            // 大集合通常是框架/基础设施事件池，静态连边精度很差。
            continue;
        }
        for (disp, line) in disps {
            for (reg, reg_line) in regs {
                if disp.id == reg.id {
                    continue;
                }
                let key = format!("{}>{}", disp.id, reg.id);
                if !seen.insert(key) {
                    continue;
                }
                edges.push(edge(
                    &disp.id,
                    &reg.id,
                    EdgeKind::Calls,
                    Some(line),
                    "closure-collection",
                    [
                        ("field", json!(field)),
                        (
                            "registeredAt",
                            json!(format!("{}:{reg_line}", reg.file_path)),
                        ),
                    ],
                ));
            }
        }
    }
    edges
}

pub(super) fn event_emitter_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    // 只处理字面量事件名和可命名 handler；匿名闭包没有稳定节点，开放式字符串事件
    // 也容易误连。
    static ON_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"\.(?:on|once|addListener)\(\s*['"]([^'"]+)['"]\s*,\s*(?:function\s+(\w+)|(?:this\.)?(\w+))"#).unwrap()
    });
    static EMIT_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"\.(?:emit|fire|dispatchEvent)\(\s*['"]([^'"]+)['"]"#).unwrap()
    });

    let mut emits_by_event: HashMap<String, HashSet<String>> = HashMap::new();
    let mut handlers_by_event: HashMap<String, HashMap<String, String>> = HashMap::new();
    for file in ctx.get_all_files() {
        let Some(content) = ctx.read_file(&file) else {
            continue;
        };
        let has_emit = content.contains(".emit(")
            || content.contains(".fire(")
            || content.contains(".dispatchEvent(");
        let has_on = content.contains(".on(")
            || content.contains(".once(")
            || content.contains(".addListener(");
        if !has_emit && !has_on {
            continue;
        }
        let nodes = ctx.get_nodes_in_file(&file);
        let line_of = |idx: usize| content[..idx].lines().count() as u64 + 1;
        if has_emit {
            for cap in EMIT_RE.captures_iter(&content) {
                if let Some(dispatcher) = enclosing_fn(&nodes, line_of(cap.get(0).unwrap().start()))
                {
                    emits_by_event
                        .entry(cap[1].to_string())
                        .or_default()
                        .insert(dispatcher.id);
                }
            }
        }
        if has_on {
            for cap in ON_RE.captures_iter(&content) {
                let handler_name = cap.get(2).or_else(|| cap.get(3)).map(|m| m.as_str());
                let Some(handler_name) = handler_name else {
                    continue;
                };
                if let Some(handler) = ctx
                    .get_nodes_by_name(handler_name)
                    .into_iter()
                    .find(|node| matches!(node.kind, NodeKind::Function | NodeKind::Method))
                {
                    handlers_by_event
                        .entry(cap[1].to_string())
                        .or_default()
                        .insert(
                            handler.id,
                            format!("{}:{}", file, line_of(cap.get(0).unwrap().start())),
                        );
                }
            }
        }
    }

    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for (event, dispatchers) in emits_by_event {
        let Some(handlers) = handlers_by_event.get(&event) else {
            continue;
        };
        if dispatchers.len() > EVENT_FANOUT_CAP || handlers.len() > EVENT_FANOUT_CAP {
            // 事件名太热时宁可不给边；错误的 event bus 全连接会比没有边更糟。
            continue;
        }
        for dispatcher in dispatchers {
            for (handler, registered_at) in handlers {
                if dispatcher == *handler {
                    continue;
                }
                let key = format!("{dispatcher}>{handler}");
                if !seen.insert(key) {
                    continue;
                }
                edges.push(edge(
                    &dispatcher,
                    handler,
                    EdgeKind::Calls,
                    None,
                    "event-emitter",
                    [
                        ("event", json!(event)),
                        ("registeredAt", json!(registered_at)),
                    ],
                ));
            }
        }
    }
    edges
}
