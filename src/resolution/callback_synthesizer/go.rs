//! Go-specific synthesized edges.
//!
//! Go 的接口实现和 receiver 方法常跨文件分散；这些 pass 同时修补确定性结构边
//! 和少量运行时接口赋值边，帮助 explore 把 interface call 追到具体实现。

use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde_json::json;

use crate::db::queries::QueryBuilder;
use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};
use crate::resolution::types::ResolutionContext;
use crate::types::{Edge, EdgeKind, Language, Node, NodeKind};

use super::common::{MAX_CALLBACKS_PER_CHANNEL, children_of_kind, edge, static_edge};

pub(super) fn go_cross_file_method_contains_edges(queries: &mut QueryBuilder) -> Vec<Edge> {
    // tree-sitter 只能看到 method 所在文件；Go receiver type 可能在同 package
    // 另一文件中声明。按同目录 receiver 名称补 contains，供 class/member 查询使用。
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    let type_kinds = HashSet::from([
        NodeKind::Struct,
        NodeKind::Class,
        NodeKind::Interface,
        NodeKind::Enum,
        NodeKind::TypeAlias,
    ]);
    let dir_of = |p: &str| p.rsplit_once('/').map(|(d, _)| d).unwrap_or("").to_string();
    for method in queries
        .get_nodes_by_kind(NodeKind::Method)
        .unwrap_or_default()
    {
        if method.language != Language::Go {
            continue;
        }
        let Some((receiver, _)) = method.qualified_name.rsplit_once("::") else {
            continue;
        };
        let has_type_parent = queries
            .get_incoming_edges(&method.id, Some(vec![EdgeKind::Contains]))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|edge| queries.get_node_by_id(&edge.source).ok().flatten())
            .any(|src| type_kinds.contains(&src.kind));
        if has_type_parent {
            continue;
        }
        let dir = dir_of(&method.file_path);
        let owner = queries
            .get_nodes_by_name(receiver)
            .unwrap_or_default()
            .into_iter()
            .find(|node| {
                node.language == Language::Go
                    && type_kinds.contains(&node.kind)
                    && dir_of(&node.file_path) == dir
            });
        let Some(owner) = owner else {
            continue;
        };
        let key = format!("{}>{}", owner.id, method.id);
        if seen.insert(key) {
            edges.push(static_edge(
                &owner.id,
                &method.id,
                EdgeKind::Contains,
                Some(method.start_line),
            ));
        }
    }
    edges
}

pub(super) fn go_implements_edges(
    queries: &mut QueryBuilder,
    ctx: &mut dyn ResolutionContext,
) -> Vec<Edge> {
    // Go 没有显式 implements；这里用“struct 方法集合覆盖 interface 方法集合”
    // 合成 implements 边，限制候选数量以避免大包里 O(N*M) 过度膨胀。
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    let mut method_names = |queries: &mut QueryBuilder, id: &str| -> Option<HashSet<String>> {
        children_of_kind(queries, id, NodeKind::Method)
            .into_iter()
            .map(|node| Some(format!("{}:{}", node.name, go_signature(&node, ctx)?)))
            .collect()
    };
    let go_structs = queries
        .get_nodes_by_kind(NodeKind::Struct)
        .unwrap_or_default()
        .into_iter()
        .filter(|node| node.language == Language::Go)
        .collect::<Vec<_>>();
    let mut struct_methods = HashMap::new();
    for node in &go_structs {
        struct_methods.insert(node.id.clone(), method_names(queries, &node.id));
    }
    for iface in queries
        .get_nodes_by_kind(NodeKind::Interface)
        .unwrap_or_default()
    {
        if iface.language != Language::Go {
            continue;
        }
        let Some(want) = method_names(queries, &iface.id) else {
            continue;
        };
        if want.is_empty() {
            continue;
        }
        for strct in go_structs.iter().take(MAX_CALLBACKS_PER_CHANNEL) {
            let Some(have) = struct_methods
                .get(&strct.id)
                .and_then(|methods| methods.as_ref())
            else {
                continue;
            };
            if want.iter().all(|name| have.contains(name)) {
                let key = format!("{}>{}", strct.id, iface.id);
                if seen.insert(key) {
                    edges.push(edge(
                        &strct.id,
                        &iface.id,
                        EdgeKind::Implements,
                        Some(strct.start_line),
                        "go-implements",
                        [
                            ("via", json!(iface.name)),
                            (
                                "registeredAt",
                                json!(format!("{}:{}", strct.file_path, strct.start_line)),
                            ),
                        ],
                    ));
                }
            }
        }
    }
    edges
}

#[derive(Debug, Clone)]
struct GoInterfaceDecl {
    name: String,
    file_path: String,
    line: u64,
    methods: HashSet<String>,
}

#[derive(Debug, Clone)]
struct GoInterfaceVar {
    name: String,
    interface_name: String,
    file_path: String,
    line: u64,
}

#[derive(Debug, Clone)]
struct GoInterfaceAssignment {
    variable_name: String,
    impl_name: String,
    file_path: String,
    line: u64,
}

pub(super) fn go_interface_assignment_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    // 只桥接形如 `var x Interface` 后面 `x = Impl{}` 的显式赋值。先验证 Impl
    // 真的满足 interface 方法集合，再从 interface/变量节点连到实现类型。
    let files = ctx
        .get_all_files()
        .into_iter()
        .filter(|file| file.ends_with(".go"))
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Vec::new();
    }

    let mut interfaces: HashMap<String, GoInterfaceDecl> = HashMap::new();
    let mut variables = Vec::new();
    let mut assignments = Vec::new();
    for file in files {
        let Some(content) = ctx.read_file(&file) else {
            continue;
        };
        let safe = strip_comments_for_regex(&content, CommentLang::Go);
        for interface in go_interface_decls(&safe, &file) {
            interfaces
                .entry(interface.name.clone())
                .or_insert(interface);
        }
        variables.extend(go_interface_vars(&safe, &file));
        assignments.extend(go_interface_assignments(&safe, &file));
    }

    let variables_by_name = variables
        .into_iter()
        .filter(|var| interfaces.contains_key(&var.interface_name))
        .map(|var| (var.name.clone(), var))
        .collect::<HashMap<_, _>>();
    if variables_by_name.is_empty() || assignments.is_empty() {
        return Vec::new();
    }

    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for assignment in assignments {
        let Some(var) = variables_by_name.get(&assignment.variable_name) else {
            continue;
        };
        let Some(interface) = interfaces.get(&var.interface_name) else {
            continue;
        };
        if interface.methods.is_empty()
            || !go_type_satisfies_methods(
                &assignment.impl_name,
                &assignment.file_path,
                interface,
                ctx,
            )
        {
            continue;
        }
        let Some(source_node) = go_interface_source_node(interface, var, ctx) else {
            continue;
        };
        let Some(target_node) = ctx
            .get_nodes_by_name(&assignment.impl_name)
            .into_iter()
            .find(|node| {
                node.language == Language::Go
                    && node.file_path == assignment.file_path
                    && matches!(node.kind, NodeKind::Struct | NodeKind::Class)
            })
        else {
            continue;
        };
        let key = format!("{}>{}", source_node.id, target_node.id);
        if seen.insert(key) {
            edges.push(edge(
                &source_node.id,
                &target_node.id,
                EdgeKind::References,
                Some(assignment.line),
                "go-interface-assignment",
                [
                    (
                        "via",
                        json!(format!(
                            "{}:{}->{}",
                            var.name, interface.name, assignment.impl_name
                        )),
                    ),
                    (
                        "registeredAt",
                        json!(format!("{}:{}", assignment.file_path, assignment.line)),
                    ),
                    (
                        "interfaceDeclaredAt",
                        json!(format!("{}:{}", interface.file_path, interface.line)),
                    ),
                    (
                        "variableDeclaredAt",
                        json!(format!("{}:{}", var.file_path, var.line)),
                    ),
                ],
            ));
        }
    }
    edges
}

fn go_interface_source_node(
    interface: &GoInterfaceDecl,
    var: &GoInterfaceVar,
    ctx: &mut dyn ResolutionContext,
) -> Option<Node> {
    ctx.get_nodes_by_name(&interface.name)
        .into_iter()
        .find(|node| {
            node.language == Language::Go
                && node.file_path == interface.file_path
                && node.kind == NodeKind::Interface
        })
        .or_else(|| {
            ctx.get_nodes_by_name(&var.name).into_iter().find(|node| {
                node.language == Language::Go
                    && node.file_path == var.file_path
                    && matches!(node.kind, NodeKind::Variable | NodeKind::Constant)
            })
        })
}

fn go_type_satisfies_methods(
    impl_name: &str,
    impl_file: &str,
    interface: &GoInterfaceDecl,
    ctx: &mut dyn ResolutionContext,
) -> bool {
    interface.methods.iter().all(|method| {
        let declarations = ctx.get_nodes_by_name(method);
        let wanted = declarations
            .iter()
            .filter(|n| {
                n.kind == NodeKind::Method
                    && n.file_path == interface.file_path
                    && go_receiver_from_qualified_name(&n.qualified_name)
                        == Some(interface.name.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        declarations
            .iter()
            .filter(|node| {
                node.language == Language::Go
                    && node.file_path == impl_file
                    && node.kind == NodeKind::Method
                    && go_receiver_from_qualified_name(&node.qualified_name) == Some(impl_name)
            })
            .any(|node| {
                let Some(have) = go_signature(node, ctx) else {
                    return false;
                };
                wanted
                    .iter()
                    .any(|want| go_signature(want, ctx).as_ref() == Some(&have))
            })
    })
}

fn go_interface_decls(source: &str, file_path: &str) -> Vec<GoInterfaceDecl> {
    // Go interface 可以单行或多行声明；这里用 brace depth 读取完整方法集合。
    static START_RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"^\s*type\s+(\w+)\s+interface\s*\{").unwrap());
    let lines = source.lines().collect::<Vec<_>>();
    let mut out = Vec::new();
    let mut idx = 0usize;
    while idx < lines.len() {
        let line = lines[idx];
        let Some(cap) = START_RE.captures(line) else {
            idx += 1;
            continue;
        };
        let name = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
        let line_number = idx as u64 + 1;
        let mut methods = HashSet::new();
        let mut depth = brace_delta_for_text(line);
        let mut scan = idx;
        while scan < lines.len() {
            let current = lines[scan].trim();
            if scan != idx {
                if let Some(method) = go_interface_method_name(current) {
                    methods.insert(method.to_string());
                }
            } else if let Some(open) = current.find('{') {
                for segment in current[open + 1..].split(';') {
                    if let Some(method) = go_interface_method_name(segment.trim()) {
                        methods.insert(method.to_string());
                    }
                }
            }
            if scan != idx {
                depth += brace_delta_for_text(lines[scan]);
            }
            if depth <= 0 {
                break;
            }
            scan += 1;
        }
        if !name.is_empty() {
            out.push(GoInterfaceDecl {
                name,
                file_path: file_path.to_string(),
                line: line_number,
                methods,
            });
        }
        idx = scan.saturating_add(1);
    }
    out
}

fn go_interface_vars(source: &str, file_path: &str) -> Vec<GoInterfaceVar> {
    static VAR_RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"(?m)^\s*var\s+(\w+)\s+(\w+)\b").unwrap());
    VAR_RE
        .captures_iter(source)
        .filter_map(|cap| {
            let name = cap.get(1)?.as_str().to_string();
            let interface_name = cap.get(2)?.as_str().to_string();
            let line = super::common::prefix_line_count(source, cap.get(0)?.start()) + 1;
            Some(GoInterfaceVar {
                name,
                interface_name,
                file_path: file_path.to_string(),
                line,
            })
        })
        .collect()
}

fn go_interface_assignments(source: &str, file_path: &str) -> Vec<GoInterfaceAssignment> {
    static ASSIGN_RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"(?m)\b(\w+)\s*=\s*([A-Za-z_]\w*)\s*\{").unwrap());
    ASSIGN_RE
        .captures_iter(source)
        .filter_map(|cap| {
            let variable_name = cap.get(1)?.as_str().to_string();
            let impl_name = cap.get(2)?.as_str().to_string();
            let line = super::common::prefix_line_count(source, cap.get(0)?.start()) + 1;
            Some(GoInterfaceAssignment {
                variable_name,
                impl_name,
                file_path: file_path.to_string(),
                line,
            })
        })
        .collect()
}

fn go_interface_method_name(line: &str) -> Option<&str> {
    let trimmed = line.trim().trim_end_matches('}').trim();
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.contains(" interface") {
        return None;
    }
    let name = trimmed.split('(').next()?.trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(name)
}

fn brace_delta_for_text(text: &str) -> isize {
    text.chars().fold(0isize, |acc, ch| match ch {
        '{' => acc + 1,
        '}' => acc - 1,
        _ => acc,
    })
}

pub(super) fn go_receiver_from_qualified_name(qualified_name: &str) -> Option<&str> {
    // Go 方法 qualified_name 约定为 `Receiver::method`；其它语言调用者会得到 None。
    let mut parts = qualified_name.rsplit("::");
    let _method = parts.next()?;
    parts.next()
}

// Compare stored Go signatures without erasing package identity. Unsupported
// compound signatures are rejected rather than guessed from method names.
fn go_signature(node: &Node, ctx: &mut dyn ResolutionContext) -> Option<String> {
    let signature = node.signature.as_deref()?;
    let (params, result) = signature.strip_prefix('(')?.split_once(')')?;
    let result = result.trim().trim_start_matches('(').trim_end_matches(')');
    let nodes = ctx.get_nodes_in_file(&node.file_path);
    let mut canonical = |items: &str| -> Option<String> {
        let mut out = Vec::new();
        let items = items
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>();
        let mut written = Vec::new();
        let mut carried = None;
        for item in items.into_iter().rev() {
            let item = item.trim();
            let ty = if let Some((name, rest)) = item.split_once(char::is_whitespace) {
                if name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
                    && !matches!(name, "chan" | "map" | "func" | "struct" | "interface")
                {
                    carried = Some(rest.trim());
                    rest.trim()
                } else {
                    item
                }
            } else if let Some(carried) = carried {
                carried
            } else {
                item
            };
            written.push(ty);
        }
        let alias_re = Regex::new(r"(?m)^\s*type\s+(\w+)\s*=\s*([\w.*\[\]]+)\s*$").ok()?;
        let tokens = Regex::new(r"[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)?").ok()?;
        for ty in written.into_iter().rev() {
            if ty.contains(['(', ')', '{', '}']) {
                return None;
            }
            let mut ty = ty
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>();
            let source = strip_comments_for_regex(
                &ctx.read_file(&node.file_path).unwrap_or_default(),
                CommentLang::Go,
            );
            // Only true aliases preserve identity; defined types remain distinct.
            for _ in 0..8 {
                let old = ty.clone();
                for cap in alias_re.captures_iter(&source) {
                    let word = Regex::new(&format!(r"\b{}\b", regex::escape(&cap[1]))).ok()?;
                    ty = word.replace_all(&ty, &cap[2]).to_string();
                }
                if ty == old {
                    break;
                }
            }
            let mut imports = HashMap::new();
            for import in nodes.iter().filter(|n| n.kind == NodeKind::Import) {
                let path = import.name.trim_matches('"');
                let alias = import
                    .signature
                    .as_deref()
                    .and_then(|s| s.split_once(' '))
                    .map(|(a, _)| a)
                    .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path));
                imports.insert(alias.to_string(), path.to_string());
            }
            let mut normalized = String::new();
            let mut previous = 0;
            for token in tokens.find_iter(&ty) {
                normalized.push_str(&ty[previous..token.start()]);
                let name = token.as_str();
                if let Some((package, member)) = name.split_once('.') {
                    normalized.push_str(&format!("[{}].{}", imports.get(package)?, member));
                } else if matches!(
                    name,
                    "bool"
                        | "string"
                        | "int"
                        | "int8"
                        | "int16"
                        | "int32"
                        | "int64"
                        | "uint"
                        | "uint8"
                        | "uint16"
                        | "uint32"
                        | "uint64"
                        | "uintptr"
                        | "float32"
                        | "float64"
                        | "complex64"
                        | "complex128"
                        | "byte"
                        | "rune"
                        | "error"
                        | "any"
                        | "map"
                        | "chan"
                ) {
                    normalized.push_str(match name {
                        "uint8" => "byte",
                        "int32" => "rune",
                        _ => name,
                    });
                } else {
                    let directory = node
                        .file_path
                        .rsplit_once('/')
                        .map(|(dir, _)| dir)
                        .unwrap_or("");
                    normalized.push_str(&format!("[{directory}].{name}"));
                }
                previous = token.end();
            }
            normalized.push_str(&ty[previous..]);
            ty = normalized;
            out.push(ty);
        }
        Some(out.join(","))
    };
    Some(format!("({})->{}", canonical(params)?, canonical(result)?))
}
