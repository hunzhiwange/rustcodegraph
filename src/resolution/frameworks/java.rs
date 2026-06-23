//! Spring / Java framework resolver translated from `java.ts`.
//!
//! Spring resolver 覆盖三条链路：控制器 mapping -> handler，`@Value`/
//! `@ConfigurationProperties` -> 配置 key，以及按目录惯例解析 Service/Repository 等类型。

use regex::Regex;

use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};
use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    line_for_byte, make_node, make_reference, resolve_by_name_and_kind,
};
use crate::types::{Language, Node, NodeKind, ReferenceKind};

const SERVICE_DIRS: &[&str] = &["/service/", "/services/"];
const REPO_DIRS: &[&str] = &["/repository/", "/repositories/"];
const CONTROLLER_DIRS: &[&str] = &["/controller/", "/controllers/"];
const ENTITY_DIRS: &[&str] = &["/entity/", "/entities/", "/model/", "/models/", "/domain/"];
const COMPONENT_DIRS: &[&str] = &["/component/", "/components/", "/config/"];
const CLASS_KINDS: &[NodeKind] = &[NodeKind::Class];
const SERVICE_KINDS: &[NodeKind] = &[NodeKind::Class, NodeKind::Interface];

pub struct SpringResolver;

pub const SPRING_RESOLVER: SpringResolver = SpringResolver;

impl FrameworkResolver for SpringResolver {
    fn name(&self) -> &'static str {
        "spring"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[
            Language::Java,
            Language::Kotlin,
            Language::Yaml,
            Language::Properties,
        ])
    }

    fn claims_reference(&self, name: &str) -> bool {
        // `prefix:prefix` 是 ConfigurationProperties 人造引用，避免普通 name matcher 接管。
        name.ends_with(":prefix")
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        // 构建文件依赖是强信号；Java 注解扫描作为没有构建文件时的兜底。
        for file in ["pom.xml", "build.gradle", "build.gradle.kts"] {
            if context.read_file(file).is_some_and(|content| {
                content.contains("spring-boot") || content.contains("springframework")
            }) {
                return true;
            }
        }
        for file in context.get_all_files() {
            if !file.ends_with(".java") {
                continue;
            }
            if let Some(content) = context.read_file(&file)
                && (content.contains("@SpringBootApplication")
                    || content.contains("@RestController")
                    || content.contains("@Service")
                    || content.contains("@Repository"))
            {
                return true;
            }
        }
        false
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        if let Some(prefix) = reference.reference_name.strip_suffix(":prefix") {
            // ConfigurationProperties prefix 解析到最短匹配配置叶子，表示这个 prefix
            // 在配置文件中存在。
            let canon_prefix = canonical_config_key(prefix);
            let mut candidates = context
                .get_nodes_by_kind(NodeKind::Constant)
                .into_iter()
                .filter(|node| matches!(node.language, Language::Yaml | Language::Properties))
                .filter(|node| {
                    canonical_config_key(&node.qualified_name).starts_with(&canon_prefix)
                })
                .collect::<Vec<_>>();
            candidates.sort_by_key(|node| canonical_config_key(&node.qualified_name).len());
            return candidates
                .first()
                .map(|node| ResolvedRef::framework(reference, node.id.clone(), 0.85));
        }

        if matches!(reference.language, Language::Java | Language::Kotlin)
            && reference.reference_name.contains('.')
            && !reference.reference_name.contains("::")
            && reference.reference_name.split('.').count() >= 2
        {
            // @Value("${foo.bar}") 用 relaxed binding 规则匹配 YAML/properties key。
            let canon_ref = canonical_config_key(&reference.reference_name);
            let mut candidates = context
                .get_nodes_by_kind(NodeKind::Constant)
                .into_iter()
                .filter(|node| matches!(node.language, Language::Yaml | Language::Properties))
                .filter(|node| canonical_config_key(&node.qualified_name) == canon_ref)
                .collect::<Vec<_>>();
            if candidates.len() == 1 {
                return Some(ResolvedRef::framework(
                    reference,
                    candidates.remove(0).id,
                    0.9,
                ));
            }
            if !candidates.is_empty() {
                // profile 文件里同名 key 可能多份；优先 application/bootstrap 基础文件。
                candidates.sort_by_key(config_profile_score);
                return Some(ResolvedRef::framework(
                    reference,
                    candidates[0].id.clone(),
                    0.75,
                ));
            }
        }

        if reference.reference_name.ends_with("Service")
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                SERVICE_KINDS,
                SERVICE_DIRS,
                context,
            )
        {
            // 下面的类型解析都带目录偏置，避免 Java 项目里大量同名 DTO/Entity/Service
            // 互相误连。
            return Some(ResolvedRef::framework(reference, target, 0.85));
        }
        if reference.reference_name.ends_with("Repository")
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                SERVICE_KINDS,
                REPO_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.85));
        }
        if reference.reference_name.ends_with("Controller")
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                CLASS_KINDS,
                CONTROLLER_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.85));
        }
        if is_pascal_word(&reference.reference_name)
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                CLASS_KINDS,
                ENTITY_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.7));
        }
        if (reference.reference_name.ends_with("Component")
            || reference.reference_name.ends_with("Config"))
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                CLASS_KINDS,
                COMPONENT_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if is_spring_config_file(file_path) {
            return extract_spring_config(file_path, content);
        }
        if !(file_path.ends_with(".java") || file_path.ends_with(".kt")) {
            return FrameworkExtractionResult::default();
        }
        let safe = strip_comments_for_regex(content, CommentLang::Java);
        let content = safe.as_str();
        let mut result = FrameworkExtractionResult::default();
        let lang = if file_path.ends_with(".kt") {
            Language::Kotlin
        } else {
            Language::Java
        };
        let class_prefix = Regex::new(r"@RequestMapping\s*\(([^)]*)\)\s*(?:@[\w.]+(?:\([^)]*\))?\s*)*(?:public\s+|final\s+|abstract\s+|open\s+|data\s+|sealed\s+)*class\b")
            .unwrap()
            .captures(content)
            .map(|caps| parse_mapping_path(caps.get(1).unwrap().as_str()))
            .unwrap_or_default();

        let mapping_re = Regex::new(
            r"@(GetMapping|PostMapping|PutMapping|PatchMapping|DeleteMapping)\b\s*(\([^)]*\))?",
        )
        .unwrap();
        let method_decl_re = Regex::new(
            r"\bfun\s+(\w+)\s*\(|\b(?:public|private|protected)\s+[^;{=]*?\s+(\w+)\s*\(",
        )
        .unwrap();
        for caps in mapping_re.captures_iter(content) {
            // Get/Post/etc mapping 直接给出 HTTP verb；类级 RequestMapping 作为路径前缀。
            let whole = caps.get(0).unwrap();
            let method = match caps.get(1).unwrap().as_str() {
                "GetMapping" => "GET",
                "PostMapping" => "POST",
                "PutMapping" => "PUT",
                "PatchMapping" => "PATCH",
                "DeleteMapping" => "DELETE",
                _ => "ANY",
            };
            let sub = caps
                .get(2)
                .map(|m| m.as_str().trim_start_matches('(').trim_end_matches(')'))
                .map(parse_mapping_path)
                .unwrap_or_default();
            let route_path = join_path(&class_prefix, &sub);
            let line = line_for_byte(content, whole.start());
            let route_id = format!("route:{file_path}:{line}:{method}:{route_path}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("{method} {route_path}"),
                format!("{file_path}::route:{route_path}"),
                file_path,
                lang,
                line,
                None,
                None,
            ));
            let tail = &content[whole.end()..(whole.end() + 600).min(content.len())];
            if let Some(method_caps) = method_decl_re.captures(tail) {
                // 注解和方法声明之间可能有其它注解/修饰符，限定窗口避免跨方法匹配。
                let handler = method_caps
                    .get(1)
                    .or_else(|| method_caps.get(2))
                    .unwrap()
                    .as_str();
                result.references.push(make_reference(
                    route_id,
                    handler,
                    ReferenceKind::References,
                    line,
                    0,
                    file_path,
                    lang,
                ));
            }
        }

        let request_mapping_re = Regex::new(r"@RequestMapping\b\s*(\([^)]*\))?").unwrap();
        let request_mapping_class_re = Regex::new(
            r"^\s*(?:@[\w.]+(?:\([^)]*\))?\s*)*(?:public\s+|final\s+|abstract\s+|open\s+|data\s+|sealed\s+)*class\b",
        )
        .unwrap();
        let request_mapping_method_re =
            Regex::new(r"method\s*=\s*(?:RequestMethod\.)?(\w+)").unwrap();
        for caps in request_mapping_re.captures_iter(content) {
            // 通用 RequestMapping 需要从 method=... 参数里推断 HTTP verb；类级注解已作为
            // class_prefix 处理，这里跳过。
            let whole = caps.get(0).unwrap();
            let args = caps
                .get(1)
                .map(|m| m.as_str().trim_start_matches('(').trim_end_matches(')'))
                .unwrap_or("");
            let after = &content[whole.end()..(whole.end() + 600).min(content.len())];
            if request_mapping_class_re.is_match(after) {
                continue;
            }
            let Some(method_caps) = method_decl_re.captures(after) else {
                continue;
            };
            let method = request_mapping_method_re
                .captures(args)
                .and_then(|caps| caps.get(1).map(|m| m.as_str().to_ascii_uppercase()))
                .unwrap_or_else(|| "ANY".to_string());
            let route_path = join_path(&class_prefix, &parse_mapping_path(args));
            let line = line_for_byte(content, whole.start());
            let route_id = format!("route:{file_path}:{line}:{method}:{route_path}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("{method} {route_path}"),
                format!("{file_path}::route:{route_path}"),
                file_path,
                lang,
                line,
                None,
                None,
            ));
            let handler = method_caps
                .get(1)
                .or_else(|| method_caps.get(2))
                .unwrap()
                .as_str();
            result.references.push(make_reference(
                route_id,
                handler,
                ReferenceKind::References,
                line,
                0,
                file_path,
                lang,
            ));
        }

        extract_spring_value_bindings(file_path, content, lang, &mut result);
        result
    }
}

fn is_spring_config_file(file_path: &str) -> bool {
    let base = file_path.rsplit('/').next().unwrap_or(file_path);
    Regex::new(r"(?i)^(application|bootstrap)(-[\w.-]+)?\.(yml|yaml|properties)$")
        .unwrap()
        .is_match(base)
}

fn extract_spring_config(file_path: &str, content: &str) -> FrameworkExtractionResult {
    // 配置 key 抽成 Constant 节点，value 只用长度存在 end_column，不把敏感值放入图。
    let mut result = FrameworkExtractionResult::default();
    let is_properties = file_path.ends_with(".properties");
    let lang = if is_properties {
        Language::Properties
    } else {
        Language::Yaml
    };
    if is_properties {
        for (idx, raw) in content.lines().enumerate() {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                continue;
            }
            let Some(sep) = raw.find(['=', ':']) else {
                continue;
            };
            let key = raw[..sep].trim();
            let value = raw[sep + 1..].trim();
            emit_config_leaf(&mut result, file_path, key, (idx + 1) as u64, value, lang);
        }
        return result;
    }

    if spring_yaml_is_flattened(content) {
        // 有些测试/生成文件把层级 YAML 展平成无缩进父子行，单独兼容。
        extract_flattened_spring_yaml_config(file_path, content, lang, &mut result);
        return result;
    }

    let mut stack: Vec<(usize, String)> = Vec::new();
    for (idx, raw) in content.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed == "---"
            || trimmed.starts_with("- ")
        {
            continue;
        }
        let indent = raw.len() - raw.trim_start_matches([' ', '\t']).len();
        let Some(colon_idx) = raw.find(':') else {
            continue;
        };
        let key = raw[indent..colon_idx].trim();
        if key.is_empty() {
            continue;
        }
        let after = raw[colon_idx + 1..].trim();
        while stack
            .last()
            .is_some_and(|(last_indent, _)| *last_indent >= indent)
        {
            // YAML 缩进回退时弹出父 key，形成 dotted key。
            stack.pop();
        }
        let dotted = stack
            .iter()
            .map(|(_, key)| key.as_str())
            .chain(std::iter::once(key))
            .collect::<Vec<_>>()
            .join(".");
        if after.is_empty() || after.starts_with('#') {
            stack.push((indent, key.to_string()));
        } else {
            emit_config_leaf(
                &mut result,
                file_path,
                &dotted,
                (idx + 1) as u64,
                after.trim_matches(|ch| ch == '"' || ch == '\''),
                lang,
            );
        }
    }
    result
}

fn spring_yaml_is_flattened(content: &str) -> bool {
    // 正常 YAML 子项会有缩进；如果同时出现无缩进 parent 和 leaf，就按 flattened
    // 模式解析，避免 stack 一直堆叠。
    let mut saw_parent = false;
    let mut saw_leaf = false;
    for raw in content.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed == "---"
            || trimmed.starts_with("- ")
        {
            continue;
        }
        let indent = raw.len() - raw.trim_start_matches([' ', '\t']).len();
        if indent > 0 {
            return false;
        }
        let Some(colon_idx) = raw.find(':') else {
            continue;
        };
        let after = raw[colon_idx + 1..].trim();
        if after.is_empty() || after.starts_with('#') {
            saw_parent = true;
        } else {
            saw_leaf = true;
        }
    }
    saw_parent && saw_leaf
}

fn extract_flattened_spring_yaml_config(
    file_path: &str,
    content: &str,
    language: Language,
    result: &mut FrameworkExtractionResult,
) {
    // flattened 模式用 previous_was_leaf 控制父 key 生命周期，模拟简单的一层缩进。
    let mut stack: Vec<String> = Vec::new();
    let mut previous_was_leaf = false;
    for (idx, raw) in content.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed == "---"
            || trimmed.starts_with("- ")
        {
            continue;
        }
        let Some(colon_idx) = raw.find(':') else {
            continue;
        };
        let key = raw[..colon_idx].trim();
        if key.is_empty() {
            continue;
        }
        let after = raw[colon_idx + 1..].trim();
        if after.is_empty() || after.starts_with('#') {
            if previous_was_leaf {
                stack.clear();
            }
            stack.push(key.to_string());
            previous_was_leaf = false;
            continue;
        }
        if previous_was_leaf && !stack.is_empty() {
            stack.pop();
        }
        let dotted = stack
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(key))
            .collect::<Vec<_>>()
            .join(".");
        emit_config_leaf(
            result,
            file_path,
            &dotted,
            (idx + 1) as u64,
            after.trim_matches(|ch| ch == '"' || ch == '\''),
            language,
        );
        previous_was_leaf = true;
    }
}

fn emit_config_leaf(
    result: &mut FrameworkExtractionResult,
    file_path: &str,
    dotted_key: &str,
    line: u64,
    value_text: &str,
    language: Language,
) {
    if dotted_key.is_empty() {
        return;
    }
    result.nodes.push(make_node(
        format!("spring-config:{file_path}:{line}:{dotted_key}"),
        NodeKind::Constant,
        dotted_key.rsplit('.').next().unwrap_or(dotted_key),
        dotted_key,
        file_path,
        language,
        line,
        Some(dotted_key.to_string()),
        None,
    ));
    if let Some(node) = result.nodes.last_mut() {
        node.end_column = value_text.len() as u64;
    }
}

fn extract_spring_value_bindings(
    file_path: &str,
    content: &str,
    language: Language,
    result: &mut FrameworkExtractionResult,
) {
    // @Value 生成一个代码侧 Constant 节点，再引用配置 key，后续 resolver 会把它连到
    // YAML/properties 中的真实 key。
    let value_re = Regex::new(r#"@Value\s*\(\s*["']\$\{([^}:]+)(?::[^}]*)?\}["']\s*\)"#).unwrap();
    for caps in value_re.captures_iter(content) {
        let whole = caps.get(0).unwrap();
        let key = caps.get(1).unwrap().as_str().trim();
        if key.is_empty() {
            continue;
        }
        let line = line_for_byte(content, whole.start());
        let id = format!("spring-value:{file_path}:{line}:{key}");
        result.nodes.push(make_node(
            id.clone(),
            NodeKind::Constant,
            key,
            format!("{file_path}::@Value:{key}"),
            file_path,
            language,
            line,
            Some(format!("@Value(\"{key}\")")),
            None,
        ));
        result.references.push(make_reference(
            id,
            key,
            ReferenceKind::References,
            line,
            0,
            file_path,
            language,
        ));
    }

    let cp_re = Regex::new(r#"@ConfigurationProperties\s*\(\s*(?:prefix\s*=\s*)?["']([^"']+)["']"#)
        .unwrap();
    for caps in cp_re.captures_iter(content) {
        // prefix 引用用特殊后缀进入 claims_reference 分支，避免和普通符号重名。
        let whole = caps.get(0).unwrap();
        let prefix = caps.get(1).unwrap().as_str().trim();
        if prefix.is_empty() {
            continue;
        }
        let line = line_for_byte(content, whole.start());
        let id = format!("spring-cp:{file_path}:{line}:{prefix}");
        result.nodes.push(make_node(
            id.clone(),
            NodeKind::Constant,
            prefix,
            format!("{file_path}::@ConfigurationProperties:{prefix}"),
            file_path,
            language,
            line,
            Some(format!("@ConfigurationProperties(\"{prefix}\")")),
            None,
        ));
        result.references.push(make_reference(
            id,
            format!("{prefix}:prefix"),
            ReferenceKind::References,
            line,
            0,
            file_path,
            language,
        ));
    }
}

fn canonical_config_key(key: &str) -> String {
    // Spring relaxed binding 忽略大小写、连字符和下划线。
    key.chars()
        .filter(|ch| *ch != '-' && *ch != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

fn config_profile_score(node: &Node) -> usize {
    // 同 key 多 profile 时，基础 application/bootstrap 配置最通用，优先显示。
    let base = node.file_path.rsplit('/').next().unwrap_or(&node.file_path);
    let is_base = Regex::new(r"(?i)^(application|bootstrap)\.(yml|yaml|properties)$")
        .unwrap()
        .is_match(base);
    (if is_base { 0 } else { 1000 }) + base.len()
}

fn parse_mapping_path(args: &str) -> String {
    Regex::new(r#"["']([^"']*)["']"#)
        .unwrap()
        .captures(args)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_default()
}

fn join_path(prefix: &str, sub: &str) -> String {
    let parts = [prefix, sub]
        .into_iter()
        .map(|part| part.trim_matches('/'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    format!("/{}", parts.join("/"))
}

fn is_pascal_word(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('A'..='Z')) && chars.all(|ch| ch.is_ascii_alphabetic())
}
