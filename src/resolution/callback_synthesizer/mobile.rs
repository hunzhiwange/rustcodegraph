//! Mobile cross-language and native bridge edge synthesis.
//!
//! React Native / Fabric / Expo 的调用链经常跨 JS、ObjC/Swift、Java/Kotlin/C++。
//! 这些 pass 只在有明确事件名、组件名或 bridge 入口时连边，避免跨平台同名方法乱连。

use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde_json::json;

use crate::db::queries::QueryBuilder;
use crate::resolution::types::ResolutionContext;
use crate::types::{Edge, EdgeKind, Language, Node, NodeKind};

use super::common::{EVENT_FANOUT_CAP, edge, enclosing_fn};

pub(super) fn rn_event_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    // 原生侧 sendEvent(name) 和 JS 侧 addListener(name, handler) 通过字面量事件名桥接。
    // 匿名 handler 退到 enclosing function，保证仍有一个稳定目标节点。
    static NATIVE_SEND_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(
            r#"sendEvent(?:WithName\s*:\s*@|\s*\(\s*withName\s*:|[^(]*\([^;{}]*?)["']([^"']+)["']"#,
        )
        .unwrap()
    });
    static ADDLISTENER_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"\.(?:on|once|addListener)\(\s*['"]([^'"]+)['"]\s*,\s*([A-Za-z_][\w.]*)"#)
            .unwrap()
    });
    let mut native_dispatchers: HashMap<String, HashSet<String>> = HashMap::new();
    let mut js_handlers: HashMap<String, HashMap<String, String>> = HashMap::new();
    for file in ctx.get_all_files() {
        let Some(content) = ctx.read_file(&file) else {
            continue;
        };
        let nodes = ctx.get_nodes_in_file(&file);
        let line_of = |idx: usize| content[..idx].lines().count() as u64 + 1;
        if file.ends_with(".m")
            || file.ends_with(".mm")
            || file.ends_with(".swift")
            || file.ends_with(".java")
            || file.ends_with(".kt")
        {
            for cap in NATIVE_SEND_RE.captures_iter(&content) {
                if let Some(dispatcher) = enclosing_fn(&nodes, line_of(cap.get(0).unwrap().start()))
                {
                    native_dispatchers
                        .entry(cap[1].to_string())
                        .or_default()
                        .insert(dispatcher.id);
                }
            }
        }
        if file.ends_with(".js")
            || file.ends_with(".jsx")
            || file.ends_with(".ts")
            || file.ends_with(".tsx")
            || file.ends_with(".mjs")
            || file.ends_with(".cjs")
        {
            for cap in ADDLISTENER_RE.captures_iter(&content) {
                let arg = cap[2].to_string();
                let bare = arg.rsplit('.').next().unwrap_or(&arg);
                let target = ctx
                    .get_nodes_by_name(bare)
                    .into_iter()
                    .find(|node| matches!(node.kind, NodeKind::Function | NodeKind::Method))
                    .or_else(|| enclosing_fn(&nodes, line_of(cap.get(0).unwrap().start())));
                if let Some(target) = target {
                    js_handlers.entry(cap[1].to_string()).or_default().insert(
                        target.id,
                        format!("{}:{}", file, line_of(cap.get(0).unwrap().start())),
                    );
                }
            }
        }
    }
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for (event, dispatchers) in native_dispatchers {
        let Some(handlers) = js_handlers.get(&event) else {
            continue;
        };
        if dispatchers.len() > EVENT_FANOUT_CAP || handlers.len() > EVENT_FANOUT_CAP {
            // 热门事件名跨平台 fanout 很容易失真，超过阈值就不合成。
            continue;
        }
        for dispatcher in dispatchers {
            for (handler, registered_at) in handlers {
                let key = format!("{dispatcher}>{handler}");
                if seen.insert(key) {
                    edges.push(edge(
                        &dispatcher,
                        handler,
                        EdgeKind::Calls,
                        None,
                        "rn-event-channel",
                        [
                            ("event", json!(event)),
                            ("registeredAt", json!(registered_at)),
                        ],
                    ));
                }
            }
        }
    }
    edges
}

pub(super) fn fabric_native_impl_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    // Fabric codegen component 节点用组件名和一组常见 native 后缀找实现类。
    let suffixes = ["", "View", "ViewManager", "ComponentView", "Manager"];
    let components = ctx
        .get_nodes_by_kind(NodeKind::Component)
        .into_iter()
        .filter(|node| node.id.starts_with("fabric-component:"))
        .collect::<Vec<_>>();
    let mut native_by_name: HashMap<String, Vec<Node>> = HashMap::new();
    for node in ctx.get_nodes_by_kind(NodeKind::Class) {
        if matches!(
            node.language,
            Language::ObjC | Language::Kotlin | Language::Java | Language::Cpp
        ) {
            native_by_name
                .entry(node.name.clone())
                .or_default()
                .push(node);
        }
    }
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for component in components {
        for suffix in suffixes {
            let candidate = format!("{}{}", component.name, suffix);
            for native in native_by_name.get(&candidate).into_iter().flatten() {
                let key = format!("{}>{}", component.id, native.id);
                if seen.insert(key) {
                    edges.push(edge(
                        &component.id,
                        &native.id,
                        EdgeKind::Calls,
                        None,
                        "fabric-native-impl",
                        [
                            (
                                "viaSuffix",
                                json!(if suffix.is_empty() { "(exact)" } else { suffix }),
                            ),
                            ("componentName", json!(component.name)),
                        ],
                    ));
                }
            }
        }
    }
    edges
}

pub(super) fn expo_cross_platform_edges(queries: &mut QueryBuilder) -> Vec<Edge> {
    // Expo Module 提取器给跨平台同一 API 生成同类 id 前缀；同一 qualified tail
    // 的不同语言实现互连，作为“这里还有另一个平台实现”的导航边。
    let mut by_key: HashMap<String, Vec<Node>> = HashMap::new();
    for method in queries
        .get_nodes_by_kind(NodeKind::Method)
        .unwrap_or_default()
    {
        if !method.id.starts_with("expo-module:") {
            continue;
        }
        if let Some(key) = method.qualified_name.split("::").last() {
            by_key.entry(key.to_string()).or_default().push(method);
        }
    }
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for group in by_key.values() {
        if group.len() < 2 {
            continue;
        }
        for a in group {
            for b in group {
                if a.id == b.id || a.language == b.language {
                    continue;
                }
                let key = format!("{}>{}", a.id, b.id);
                if seen.insert(key) {
                    edges.push(edge(
                        &a.id,
                        &b.id,
                        EdgeKind::Calls,
                        Some(a.start_line),
                        "expo-cross-platform",
                        [("via", json!(a.name))],
                    ));
                }
            }
        }
    }
    edges
}

pub(super) fn rn_cross_platform_edges(queries: &mut QueryBuilder) -> Vec<Edge> {
    // 只从已经被 JS 调到的 native bridge 方法出发，再连到其它 native sibling；
    // 这样同名生命周期/基础设施方法不会全仓互连。
    let native = HashSet::from([
        Language::Java,
        Language::Kotlin,
        Language::ObjC,
        Language::Cpp,
    ]);
    let js = HashSet::from([
        Language::TypeScript,
        Language::Tsx,
        Language::JavaScript,
        Language::Jsx,
    ]);
    let infra = HashSet::from([
        "addListener",
        "removeListeners",
        "getConstants",
        "constantsToExport",
        "getName",
        "invalidate",
        "initialize",
        "getDefaultEventTypes",
        "supportedEvents",
        "requiresMainQueueSetup",
        "methodQueue",
    ]);
    let norm = |name: &str| name.split(':').next().unwrap_or(name).to_string();
    let mut by_name: HashMap<String, Vec<Node>> = HashMap::new();
    for method in queries
        .get_nodes_by_kind(NodeKind::Method)
        .unwrap_or_default()
    {
        if native.contains(&method.language) {
            by_name.entry(norm(&method.name)).or_default().push(method);
        }
    }
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for (name, group) in by_name {
        if infra.contains(name.as_str()) {
            continue;
        }
        let langs = group
            .iter()
            .map(|node| node.language)
            .collect::<HashSet<_>>();
        if langs.len() < 2 {
            continue;
        }
        for method in &group {
            // 必须有 JS incoming call，才说明这个 native 方法是 bridge surface。
            let incoming = queries
                .get_incoming_edges(&method.id, Some(vec![EdgeKind::Calls]))
                .unwrap_or_default();
            let source_ids = incoming
                .iter()
                .map(|edge| edge.source.clone())
                .collect::<Vec<_>>();
            let sources = queries.get_nodes_by_ids(&source_ids).unwrap_or_default();
            let is_bridge = incoming.iter().any(|edge| {
                sources
                    .get(&edge.source)
                    .map(|node| js.contains(&node.language))
                    .unwrap_or(false)
            });
            if !is_bridge {
                continue;
            }
            for sibling in &group {
                if sibling.id == method.id || sibling.language == method.language {
                    continue;
                }
                for (a, b) in [(method, sibling), (sibling, method)] {
                    let key = format!("{}>{}", a.id, b.id);
                    if seen.insert(key) {
                        edges.push(edge(
                            &a.id,
                            &b.id,
                            EdgeKind::Calls,
                            Some(a.start_line),
                            "rn-cross-platform",
                            [("via", json!(name))],
                        ));
                    }
                }
            }
        }
    }
    edges
}
