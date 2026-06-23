//! Template and paired-file synthesis.
//!
//! 模板语言里组件引用、事件 handler 和代码文件配对常不表现为普通调用。这里把
//! 高置信命名关系补成图边，让 explore 能跨过 markup/runtime 边界。

use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde_json::{Value, json};

use crate::resolution::types::ResolutionContext;
use crate::types::{Edge, EdgeKind, Node, NodeKind};

use super::common::edge;

fn kebab_to_pascal(input: &str) -> String {
    input
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect()
}

fn nuxt_component_name(file_path: &str) -> Option<String> {
    // Nuxt 自动导入会把 components/foo/bar.vue 暴露成 FooBar；重复路径段需要折叠，
    // 例如 base/BaseButton.vue -> BaseButton。
    let marker = file_path.rfind("components/")?;
    let rel = file_path[marker + "components/".len()..]
        .trim_end_matches(".vue")
        .trim_end_matches(".tsx")
        .trim_end_matches(".ts")
        .trim_end_matches(".jsx")
        .trim_end_matches(".js");
    let segments = rel
        .split('/')
        .filter(|s| !s.is_empty())
        .map(kebab_to_pascal)
        .collect::<Vec<_>>();
    if segments.len() < 2 {
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    for segment in segments {
        if out
            .last()
            .map(|prev| segment.starts_with(prev))
            .unwrap_or(false)
        {
            *out.last_mut().unwrap() = segment;
        } else {
            out.push(segment);
        }
    }
    Some(out.join(""))
}

pub(super) fn vue_template_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    // Vue SFC 模板里同时处理子组件 tag 和事件 handler。handler 只接受简单命名表达式；
    // inline arrow/$event 这类无稳定节点的表达式跳过。
    static TEMPLATE_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"(?is)<template[^>]*>([\s\S]*)</template>"#).unwrap()
    });
    static VUE_KEBAB_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"<([a-z][a-z0-9]*(?:-[a-z0-9]+)+)[\s/>]"#).unwrap()
    });
    static VUE_PASCAL_RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r#"<([A-Z][A-Za-z0-9]*)[\s/>]"#).unwrap());
    static VUE_HANDLER_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"(?:@|v-on:)([a-zA-Z][\w-]*)(?:\.[\w]+)*\s*=\s*"([^"]+)""#).unwrap()
    });
    static VUE_DESTRUCTURE_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"(?:const|let|var)\s*\{([^}]+)\}\s*=\s*(\w+)\s*\("#).unwrap()
    });
    static SCRIPT_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"(?is)<script[^>]*>([\s\S]*?)</script>"#).unwrap()
    });
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    let mut nuxt_components = HashMap::new();
    for c in ctx.get_nodes_by_kind(NodeKind::Component) {
        if let Some(name) = nuxt_component_name(&c.file_path) {
            nuxt_components.entry(name).or_insert(c);
        }
    }

    for file in ctx
        .get_all_files()
        .into_iter()
        .filter(|f| f.ends_with(".vue"))
    {
        let Some(content) = ctx.read_file(&file) else {
            continue;
        };
        let Some(tpl) = TEMPLATE_RE
            .captures(&content)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
        else {
            continue;
        };
        let Some(component) = ctx
            .get_nodes_in_file(&file)
            .into_iter()
            .find(|node| node.kind == NodeKind::Component)
        else {
            continue;
        };
        let script = SCRIPT_RE
            .captures(&content)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .unwrap_or("");
        let mut destructured = HashMap::new();
        for cap in VUE_DESTRUCTURE_RE.captures_iter(script) {
            if !cap[2].starts_with("use") {
                continue;
            }
            // `const { save: submit } = useThing()` 让模板里的 submit 需要回到
            // composable 返回对象里的 save 节点。
            for part in cap[1].split(',') {
                if let Some((key, alias)) = part.trim().split_once(':') {
                    destructured.insert(
                        alias.trim().to_string(),
                        (cap[2].to_string(), key.trim().to_string()),
                    );
                } else {
                    let key = part.trim();
                    if !key.is_empty() {
                        destructured.insert(key.to_string(), (cap[2].to_string(), key.to_string()));
                    }
                }
            }
        }

        let mut add_edge = |target: Option<Node>, meta: HashMap<&'static str, Value>| {
            // 同一组件可能通过 kebab/Pascal 两种写法命中同一目标；用 source/target
            // 和 synthesizedBy 去重，保留不同类型边的语义。
            let Some(target) = target else {
                return;
            };
            if target.id == component.id {
                return;
            }
            let key = format!(
                "{}>{}>{:?}",
                component.id,
                target.id,
                meta.get("synthesizedBy")
            );
            if seen.insert(key) {
                let synth = meta
                    .get("synthesizedBy")
                    .and_then(Value::as_str)
                    .unwrap_or("jsx-render")
                    .to_string();
                edges.push(edge(
                    &component.id,
                    &target.id,
                    EdgeKind::Calls,
                    Some(component.start_line),
                    &synth,
                    meta,
                ));
            }
        };

        fn resolve_vue_name(
            ctx: &mut dyn ResolutionContext,
            file: &str,
            name: &str,
            kinds: &[NodeKind],
        ) -> Option<Node> {
            let matches = ctx
                .get_nodes_by_name(name)
                .into_iter()
                .filter(|node| kinds.contains(&node.kind))
                .collect::<Vec<_>>();
            matches
                .iter()
                .find(|node| node.file_path == file)
                .cloned()
                .or_else(|| matches.into_iter().next())
        }

        for cap in VUE_KEBAB_RE.captures_iter(&tpl) {
            let tag = kebab_to_pascal(&cap[1]);
            add_edge(
                resolve_vue_name(
                    ctx,
                    file.as_str(),
                    &tag,
                    &[NodeKind::Component, NodeKind::Function, NodeKind::Class],
                )
                .or_else(|| nuxt_components.get(&tag).cloned()),
                HashMap::from([
                    ("synthesizedBy", json!("jsx-render")),
                    ("via", json!(cap[1].to_string())),
                ]),
            );
        }
        for cap in VUE_PASCAL_RE.captures_iter(&tpl) {
            let tag = cap[1].to_string();
            add_edge(
                resolve_vue_name(
                    ctx,
                    file.as_str(),
                    &tag,
                    &[NodeKind::Component, NodeKind::Function, NodeKind::Class],
                )
                .or_else(|| nuxt_components.get(&tag).cloned()),
                HashMap::from([("synthesizedBy", json!("jsx-render")), ("via", json!(tag))]),
            );
        }
        for cap in VUE_HANDLER_RE.captures_iter(&tpl) {
            let event = cap[1].to_string();
            let expr = cap[2].trim();
            if expr.contains("=>") || expr.starts_with('$') {
                continue;
            }
            let name = expr
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect::<String>();
            if name.is_empty() {
                continue;
            }
            if let Some(direct) = resolve_vue_name(
                ctx,
                file.as_str(),
                &name,
                &[NodeKind::Method, NodeKind::Function],
            ) {
                add_edge(
                    Some(direct),
                    HashMap::from([
                        ("synthesizedBy", json!("vue-handler")),
                        ("event", json!(event)),
                    ]),
                );
                continue;
            }
            if let Some((composable, key)) = destructured.get(&name).cloned() {
                let composable_node = resolve_vue_name(
                    ctx,
                    file.as_str(),
                    &composable,
                    &[NodeKind::Method, NodeKind::Function],
                );
                let key_fn = composable_node.and_then(|node| {
                    ctx.get_nodes_by_name(&key).into_iter().find(|n| {
                        matches!(
                            n.kind,
                            NodeKind::Method
                                | NodeKind::Function
                                | NodeKind::Variable
                                | NodeKind::Constant
                        ) && n.file_path == node.file_path
                    })
                });
                add_edge(
                    key_fn,
                    HashMap::from([
                        ("synthesizedBy", json!("vue-handler")),
                        ("event", json!(event)),
                        ("via", json!(composable)),
                    ]),
                );
            }
        }
    }
    edges
}

pub(super) fn pascal_form_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    // Delphi/FMX 设计器文件和 .pas code-behind 同名配对；这是引用关系，不是调用。
    let all_files = ctx.get_all_files().into_iter().collect::<HashSet<_>>();
    let mut edges = Vec::new();
    for file in &all_files {
        if !file.ends_with(".dfm") && !file.ends_with(".fmx") {
            continue;
        }
        let pas_file = file
            .trim_end_matches(".dfm")
            .trim_end_matches(".fmx")
            .to_string()
            + ".pas";
        if !all_files.contains(&pas_file) {
            continue;
        }
        let form_node = ctx
            .get_nodes_in_file(file)
            .into_iter()
            .find(|node| node.kind == NodeKind::File);
        let unit_node = ctx
            .get_nodes_in_file(&pas_file)
            .into_iter()
            .find(|node| node.kind == NodeKind::File);
        if let (Some(unit), Some(form)) = (unit_node, form_node) {
            edges.push(edge(
                &unit.id,
                &form.id,
                EdgeKind::References,
                Some(unit.start_line),
                "pascal-form",
                [("registeredAt", json!(pas_file))],
            ));
        }
    }
    edges
}

pub(super) fn svelte_kit_load_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    // SvelteKit 的 +page/+layout 与同目录 load/actions 文件通过命名约定配对。
    let all_files = ctx.get_all_files().into_iter().collect::<HashSet<_>>();
    let mut edges = Vec::new();
    for file in &all_files {
        let Some((dir, name)) = file.rsplit_once('/') else {
            continue;
        };
        if !(name == "+page.svelte" || name == "+layout.svelte") {
            continue;
        }
        let prefix = name.trim_end_matches(".svelte");
        let Some(page) = ctx
            .get_nodes_in_file(file)
            .into_iter()
            .find(|node| node.kind == NodeKind::Component)
        else {
            continue;
        };
        for ext in [".server.ts", ".server.js", ".ts", ".js"] {
            let loader_file = format!("{dir}/{prefix}{ext}");
            if !all_files.contains(&loader_file) {
                continue;
            }
            for hook in ctx.get_nodes_in_file(&loader_file) {
                if !matches!(
                    hook.kind,
                    NodeKind::Function | NodeKind::Method | NodeKind::Constant | NodeKind::Variable
                ) || !(hook.name == "load" || hook.name == "actions")
                {
                    continue;
                }
                edges.push(edge(
                    &page.id,
                    &hook.id,
                    EdgeKind::References,
                    Some(page.start_line),
                    "sveltekit-load",
                    [
                        ("via", json!(hook.name)),
                        (
                            "registeredAt",
                            json!(format!("{}:{}", loader_file, hook.start_line)),
                        ),
                    ],
                ));
            }
        }
    }
    edges
}
