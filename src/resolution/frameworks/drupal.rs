//! Drupal framework resolver translated from `drupal.ts`.
//!
//! Drupal 的控制流大量来自 YAML routing 和 hook 命名约定。这里把 route handler、
//! FQCN controller 引用和 `hook_*` 实现补成可解析引用。

use regex::Regex;

use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    make_node, make_reference,
};
use crate::types::{Language, NodeKind, ReferenceKind};

const HOOK_FILE_EXTENSIONS: &[&str] = &[".module", ".install", ".theme", ".inc"];

pub struct DrupalResolver;

pub const DRUPAL_RESOLVER: DrupalResolver = DrupalResolver;

impl FrameworkResolver for DrupalResolver {
    fn name(&self) -> &'static str {
        "drupal"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Php, Language::Yaml])
    }

    fn claims_reference(&self, name: &str) -> bool {
        // Drupal 引用常不是普通 import：可能是 hook 名、FQCN 字符串或 YAML controller。
        name.starts_with("hook_") || name.contains('\\') || looks_like_controller_ref(name)
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        // composer type/dependency 是强信号；否则用 info.yml + routing/module 文件组合判断。
        if let Some(composer) = context.read_file("composer.json")
            && (composer.contains("\"drupal/")
                || composer.contains("\"type\"") && composer.contains("drupal-"))
        {
            return true;
        }
        let files = context.get_all_files();
        files.iter().any(|file| file.ends_with(".info.yml"))
            && files.iter().any(|file| {
                file.ends_with(".routing.yml")
                    || file.ends_with(".module")
                    || file.ends_with(".install")
                    || file.ends_with(".theme")
            })
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        let name = reference.reference_name.as_str();
        if let Some((class_name, method_name)) = parse_drupal_controller_ref(name) {
            // `_controller: Foo\Bar::baz` 优先解析到 method，找不到 method 时退到 class。
            for class_node in context.get_nodes_by_name(&class_name) {
                if class_node.kind != NodeKind::Class {
                    continue;
                }
                if let Some(method) = context
                    .get_nodes_in_file(&class_node.file_path)
                    .into_iter()
                    .find(|node| node.kind == NodeKind::Method && node.name == method_name)
                {
                    return Some(ResolvedRef::framework(reference, method.id, 0.9));
                }
                return Some(ResolvedRef::framework(reference, class_node.id, 0.7));
            }
        }
        if name.contains('\\')
            && !name.contains(':')
            && let Some(class_name) = last_segment(name)
            && let Some(class_node) = context
                .get_nodes_by_name(&class_name)
                .into_iter()
                .find(|node| node.kind == NodeKind::Class)
        {
            return Some(ResolvedRef::framework(reference, class_node.id, 0.85));
        }
        if let Some(hook_suffix) = name.strip_prefix("hook_")
            && let Some(candidate) = context
                .get_nodes_by_kind(NodeKind::Function)
                .into_iter()
                .find(|node| {
                    node.name.ends_with(&format!("_{hook_suffix}"))
                        && is_drupal_hook_file(&node.file_path)
                })
        {
            // `hook_form_alter` 解析到 `mymodule_form_alter` 这类具体模块函数。
            return Some(ResolvedRef::framework(reference, candidate.id, 0.75));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if file_path.ends_with(".routing.yml") {
            return extract_drupal_routes(file_path, content);
        }
        if is_drupal_hook_file(file_path) || file_path.ends_with(".php") {
            return extract_drupal_hooks(file_path, content);
        }
        FrameworkExtractionResult::default()
    }
}

fn extract_drupal_routes(file_path: &str, content: &str) -> FrameworkExtractionResult {
    // Drupal routing.yml 是 route-name 顶层表，path/defaults/methods 分散在子项里；
    // 用 pending route + flush 状态机避免依赖完整 YAML 解析器。
    let mut result = FrameworkExtractionResult::default();
    let mut pending: Option<(String, u64)> = None;
    let mut current_path: Option<String> = None;
    let mut handlers: Vec<String> = Vec::new();
    let mut methods: Vec<String> = Vec::new();

    let flush = |pending: &Option<(String, u64)>,
                 current_path: &Option<String>,
                 handlers: &[String],
                 methods: &[String],
                 result: &mut FrameworkExtractionResult| {
        let (route_name, line) = match pending {
            Some(pending) => pending,
            None => return,
        };
        let route_path = match current_path {
            Some(path) => path,
            None => return,
        };
        let method_tag = if methods.is_empty() {
            String::new()
        } else {
            format!(" [{}]", methods.join(","))
        };
        let route_id = format!("route:{file_path}:{line}:{route_path}");
        result.nodes.push(make_node(
            route_id.clone(),
            NodeKind::Route,
            format!("{route_path}{method_tag}"),
            format!("{file_path}::{route_name}"),
            file_path,
            Language::Yaml,
            *line,
            None,
            None,
        ));
        for handler in handlers {
            result.references.push(make_reference(
                route_id.clone(),
                handler,
                ReferenceKind::References,
                *line,
                0,
                file_path,
                Language::Yaml,
            ));
        }
    };

    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let starts_with_space = line.chars().next().is_some_and(|ch| ch.is_whitespace());
        if !starts_with_space && trimmed.ends_with(':') {
            // 新顶层 route 开始前，先提交上一条 route。
            flush(&pending, &current_path, &handlers, &methods, &mut result);
            pending = Some((trimmed.trim_end_matches(':').to_string(), (idx + 1) as u64));
            current_path = None;
            handlers.clear();
            methods.clear();
            continue;
        }
        if let Some(value) = yaml_value(trimmed, "path") {
            current_path = Some(value);
        } else if let Some(value) = yaml_value(trimmed, "_controller") {
            handlers.push(value);
        } else if let Some(value) = yaml_value(trimmed, "_form") {
            handlers.push(value);
        } else if trimmed.starts_with("_entity_form:")
            || trimmed.starts_with("_entity_list:")
            || trimmed.starts_with("_entity_view:")
        {
            if let Some((_, value)) = trimmed.split_once(':') {
                handlers.push(clean_yaml_scalar(value));
            }
        } else if let Some(value) = yaml_value(trimmed, "methods") {
            methods = value
                .trim_matches(|ch| ch == '[' || ch == ']')
                .split(',')
                .map(|method| method.trim().to_ascii_uppercase())
                .filter(|method| !method.is_empty())
                .collect();
        }
    }
    flush(&pending, &current_path, &handlers, &methods, &mut result);
    result
}

fn extract_drupal_hooks(file_path: &str, content: &str) -> FrameworkExtractionResult {
    // 先识别 docblock `Implements hook_x()`，再用模块名前缀约定兜底。
    let mut result = FrameworkExtractionResult::default();
    let func_re = Regex::new(r"(?m)^function\s+(\w+)\s*\(").unwrap();
    let mut functions = Vec::<(String, u64)>::new();
    for caps in func_re.captures_iter(content) {
        let whole = caps.get(0).unwrap();
        functions.push((
            caps.get(1).unwrap().as_str().to_string(),
            line_for_byte(content, whole.start()),
        ));
    }

    let doc_re = Regex::new(r"(?s)/\*\*.*?(?:@|\*\s+)Implements\s+(hook_\w+)\s*\(\).*?\*/\s*\n(?:\s*\n)*function\s+(\w+)\s*\(").unwrap();
    let mut matched = Vec::new();
    for caps in doc_re.captures_iter(content) {
        let hook = caps.get(1).unwrap().as_str();
        let func = caps.get(2).unwrap().as_str();
        emit_hook_ref(file_path, hook, func, &functions, &mut result);
        matched.push(func.to_string());
    }

    if let Some(module_name) = module_name_from_path(file_path) {
        let prefix = format!("{module_name}_");
        for (func_name, _) in &functions {
            if matched.iter().any(|matched| matched == func_name) || !func_name.starts_with(&prefix)
            {
                continue;
            }
            let suffix = &func_name[prefix.len()..];
            if suffix.is_empty() {
                continue;
            }
            emit_hook_ref(
                file_path,
                &format!("hook_{suffix}"),
                func_name,
                &functions,
                &mut result,
            );
        }
    }
    result
}

fn emit_hook_ref(
    file_path: &str,
    hook_name: &str,
    func_name: &str,
    functions: &[(String, u64)],
    result: &mut FrameworkExtractionResult,
) {
    if let Some((_, line)) = functions.iter().find(|(name, _)| name == func_name) {
        result.references.push(make_reference(
            format!("{file_path}::function:{func_name}:{line}"),
            hook_name,
            ReferenceKind::References,
            *line,
            0,
            file_path,
            Language::Php,
        ));
    }
}

fn yaml_value(line: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    line.strip_prefix(&prefix).map(clean_yaml_scalar)
}

fn clean_yaml_scalar(value: &str) -> String {
    // 只做 route 文件需要的标量清洗：去行尾注释和包裹引号。
    value
        .split('#')
        .next()
        .unwrap_or(value)
        .trim()
        .trim_matches(|ch| ch == '\'' || ch == '"')
        .to_string()
}

fn last_segment(fqcn: &str) -> Option<String> {
    let clean = fqcn.trim_start_matches('\\').trim();
    clean
        .contains('\\')
        .then(|| clean.rsplit('\\').next().unwrap_or(clean).to_string())
}

fn module_name_from_path(file_path: &str) -> Option<String> {
    let base = file_path.rsplit('/').next()?;
    base.split('.')
        .next()
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
}

fn is_drupal_hook_file(file_path: &str) -> bool {
    HOOK_FILE_EXTENSIONS
        .iter()
        .any(|ext| file_path.ends_with(ext))
}

fn parse_drupal_controller_ref(name: &str) -> Option<(String, String)> {
    // Drupal 支持 `Class::method` 和 legacy `Class:method` 两种写法。
    let trimmed = name.trim_start_matches('\\');
    let (class, method) = if let Some((class, method)) = trimmed.rsplit_once("::") {
        (class, method)
    } else {
        trimmed.rsplit_once(':')?
    };
    Some((
        class.rsplit('\\').next().unwrap_or(class).to_string(),
        method.to_string(),
    ))
}

fn looks_like_controller_ref(name: &str) -> bool {
    name.contains("::")
        || name
            .rsplit_once(':')
            .is_some_and(|(left, right)| !left.is_empty() && !right.is_empty())
}

fn line_for_byte(content: &str, offset: usize) -> u64 {
    (content[..offset.min(content.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1) as u64
}
