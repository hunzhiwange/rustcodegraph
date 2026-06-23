//! Play Framework resolver translated from `play.ts`.
//!
//! Play 的入口集中在 `conf/routes`，这里把文本路由行转成 route 节点并指向
//! `Controller.method`，补上配置文件到 Java/Scala 控制器的静态边。

use regex::Regex;

use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    make_node, make_reference,
};
use crate::types::{Language, NodeKind, ReferenceKind};

const METHOD_KINDS: &[NodeKind] = &[NodeKind::Method, NodeKind::Function];

pub struct PlayResolver;

pub const PLAY_RESOLVER: PlayResolver = PlayResolver;

impl FrameworkResolver for PlayResolver {
    fn name(&self) -> &'static str {
        "play"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Scala, Language::Java, Language::Yaml])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        context.read_file("build.sbt").is_some_and(|content| {
            content.contains("playframework")
                || content.contains("PlayScala")
                || content.contains("PlayJava")
        }) || context.file_exists("conf/routes")
            || context.file_exists("conf/application.conf")
    }

    fn claims_reference(&self, name: &str) -> bool {
        let Some((class_name, method_name)) = name.split_once('.') else {
            return false;
        };
        is_ident(class_name) && is_ident(method_name)
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        let (class_name, method_name) = reference.reference_name.split_once('.')?;
        for class_node in context
            .get_nodes_by_name(class_name)
            .into_iter()
            .filter(|node| node.kind == NodeKind::Class)
        {
            if let Some(method) = context
                .get_nodes_in_file(&class_node.file_path)
                .into_iter()
                .find(|node| METHOD_KINDS.contains(&node.kind) && node.name == method_name)
            {
                return Some(ResolvedRef::framework(reference, method.id, 0.9));
            }
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !is_play_routes_file(file_path) {
            return FrameworkExtractionResult::default();
        }
        // Play routes 文件不是普通 Scala/Java 源码，只能按行解析；跳过反向路由
        // 和注释行，避免把生成器语法当成真实 handler。
        let route_re =
            Regex::new(r"^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\s+(\S+)\s+(.+)$").unwrap();
        let mut result = FrameworkExtractionResult::default();
        for (idx, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("->") {
                continue;
            }
            let Some(caps) = route_re.captures(line) else {
                continue;
            };
            let method = caps.get(1).unwrap().as_str();
            let route_path = caps.get(2).unwrap().as_str();
            let action = caps.get(3).unwrap().as_str();
            let fqn = action.split('(').next().unwrap_or("").trim();
            let parts = fqn
                .split('.')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            if parts.len() < 2 {
                continue;
            }
            let handler_ref = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
            let line_num = (idx + 1) as u64;
            let route_id = format!("route:{file_path}:{line_num}:{method}:{route_path}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("{method} {route_path}"),
                format!("{file_path}::{method}:{route_path}"),
                file_path,
                Language::Scala,
                line_num,
                None,
                None,
            ));
            result.references.push(make_reference(
                route_id,
                handler_ref,
                ReferenceKind::References,
                line_num,
                0,
                file_path,
                Language::Scala,
            ));
        }
        result
    }
}

fn is_play_routes_file(file_path: &str) -> bool {
    file_path == "conf/routes" || (file_path.starts_with("conf/") && file_path.ends_with(".routes"))
}

fn is_ident(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
