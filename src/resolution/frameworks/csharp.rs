//! ASP.NET / C# framework resolver translated from `csharp.ts`.
//!
//! 这里覆盖 ASP.NET MVC/Web API 的控制器属性路由和 Minimal API `MapGet` 等调用。
//! resolver 还用目录偏置把 Service/Repository/Model 这类惯例名解析到更可信文件。

use regex::Regex;

use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};
use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    line_for_byte, make_node, make_reference, resolve_by_name_and_kind,
};
use crate::types::{Language, NodeKind, ReferenceKind};

const CONTROLLER_DIRS: &[&str] = &["/Controllers/"];
const SERVICE_DIRS: &[&str] = &["/Services/", "/Service/", "/Application/"];
const REPO_DIRS: &[&str] = &[
    "/Repositories/",
    "/Repository/",
    "/Data/",
    "/Infrastructure/",
];
const MODEL_DIRS: &[&str] = &["/Models/", "/Model/", "/Entities/", "/Entity/", "/Domain/"];
const VIEWMODEL_DIRS: &[&str] = &["/ViewModels/", "/ViewModel/", "/DTOs/", "/Dto/"];
const CLASS_KINDS: &[NodeKind] = &[NodeKind::Class];
const SERVICE_KINDS: &[NodeKind] = &[NodeKind::Class, NodeKind::Interface];

pub struct AspNetResolver;

pub const ASPNET_RESOLVER: AspNetResolver = AspNetResolver;

impl FrameworkResolver for AspNetResolver {
    fn name(&self) -> &'static str {
        "aspnet"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::CSharp])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        // csproj SDK/包引用是最强信号；随后用 Program/Startup 和 controller 语法兜底。
        for file in context.get_all_files() {
            if file.ends_with(".csproj")
                && let Some(content) = context.read_file(&file)
                && (content.contains("Microsoft.AspNetCore")
                    || content.contains("Microsoft.NET.Sdk.Web")
                    || content.contains("System.Web.Mvc"))
            {
                return true;
            }
        }

        if let Some(program) = context.read_file("Program.cs")
            && (program.contains("WebApplication")
                || program.contains("CreateHostBuilder")
                || program.contains("UseStartup"))
        {
            return true;
        }
        if context.file_exists("Startup.cs") {
            return true;
        }

        for file in context.get_all_files() {
            if !(file.ends_with("Controller.cs")
                || file.ends_with("Program.cs")
                || file.ends_with("Startup.cs"))
            {
                continue;
            }
            if let Some(content) = context.read_file(&file)
                && (content.contains("[ApiController")
                    || content.contains("[Route")
                    || content.contains("[HttpGet")
                    || content.contains("[HttpPost")
                    || content.contains("[HttpPut")
                    || content.contains("[HttpPatch")
                    || content.contains("[HttpDelete")
                    || content.contains("ControllerBase")
                    || content.contains(": Controller")
                    || content.contains("MapControllers")
                    || content.contains("WebApplication")
                    || content.contains("Microsoft.AspNetCore"))
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
        // C# 项目同名 class 常见，按目录类别做偏置，避免 Model/Service 同名误连。
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
        if (reference.reference_name.ends_with("Service")
            || (reference.reference_name.starts_with('I') && reference.reference_name.len() > 1))
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                SERVICE_KINDS,
                SERVICE_DIRS,
                context,
            )
        {
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
        if is_pascal_word(&reference.reference_name)
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                CLASS_KINDS,
                MODEL_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.7));
        }
        if (reference.reference_name.ends_with("ViewModel")
            || reference.reference_name.ends_with("Dto"))
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                CLASS_KINDS,
                VIEWMODEL_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !file_path.ends_with(".cs") {
            return FrameworkExtractionResult::default();
        }
        // 注释里的路由示例不应生成 route 节点。
        let safe = strip_comments_for_regex(content, CommentLang::CSharp);
        let mut result = FrameworkExtractionResult::default();

        let class_prefix = Regex::new(r#"\[Route\s*\(\s*"([^"]+)""#)
            .ok()
            .and_then(|re| re.captures(&safe))
            .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
            .unwrap_or_default();

        let attr_re = Regex::new(r#"\[(HttpGet|HttpPost|HttpPut|HttpPatch|HttpDelete)(?:\s*\(\s*"([^"]+)"[^)]*\))?\s*\]"#).unwrap();
        let method_re =
            Regex::new(r"(?:public|private|protected|internal)\s+[\w<>,\s\[\]?.]+?\s+(\w+)\s*\(")
                .unwrap();
        for caps in attr_re.captures_iter(&safe) {
            // `[Route]` 类前缀 + `[HttpGet("x")]` 方法后缀合成最终路径。
            let whole = caps.get(0).unwrap();
            let verb = caps.get(1).unwrap().as_str();
            let method = verb.trim_start_matches("Http").to_ascii_uppercase();
            let route_path =
                join_cs_path(&class_prefix, caps.get(2).map(|m| m.as_str()).unwrap_or(""));
            let line = line_for_byte(&safe, whole.start());
            let route_id = format!("route:{file_path}:{line}:{method}:{route_path}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("{method} {route_path}"),
                format!("{file_path}::route:{route_path}"),
                file_path,
                Language::CSharp,
                line,
                None,
                None,
            ));
            let tail_end = (whole.end() + 600).min(safe.len());
            if let Some(method_caps) = method_re.captures(&safe[whole.end()..tail_end]) {
                // 属性和方法声明之间可能有其它 attribute；限定窗口避免跨到下一个方法。
                let handler = method_caps.get(1).unwrap().as_str();
                result.references.push(make_reference(
                    route_id,
                    handler,
                    ReferenceKind::References,
                    line,
                    0,
                    file_path,
                    Language::CSharp,
                ));
            }
        }

        let minimal_re =
            Regex::new(r#"\.Map(Get|Post|Put|Patch|Delete)\s*\(\s*"([^"]+)"\s*,\s*([^,)]+)"#)
                .unwrap();
        for caps in minimal_re.captures_iter(&safe) {
            // Minimal API 的最后一个参数通常是 handler/lambda。这里只提取可命名 handler，
            // lambda 体交给常规调用抽取。
            let whole = caps.get(0).unwrap();
            let method = caps.get(1).unwrap().as_str().to_ascii_uppercase();
            let route_path = caps.get(2).unwrap().as_str();
            let line = line_for_byte(&safe, whole.start());
            let route_id = format!("route:{file_path}:{line}:{method}:{route_path}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("{method} {route_path}"),
                format!("{file_path}::route:{route_path}"),
                file_path,
                Language::CSharp,
                line,
                None,
                None,
            ));
            if let Some(handler) = extract_csharp_tail_ident(caps.get(3).unwrap().as_str()) {
                result.references.push(make_reference(
                    route_id,
                    handler,
                    ReferenceKind::References,
                    line,
                    0,
                    file_path,
                    Language::CSharp,
                ));
            }
        }

        result
    }
}

fn join_cs_path(prefix: &str, sub: &str) -> String {
    // ASP.NET 路由片段是否带 `/` 都统一成根路径。
    let parts = [prefix, sub]
        .into_iter()
        .map(|part| part.trim_matches('/'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    format!("/{}", parts.join("/"))
}

fn extract_csharp_tail_ident(expr: &str) -> Option<String> {
    // `Controller.Action`, `Handlers.Foo()` 这类表达式只需要最后的可解析标识符。
    let cleaned = expr
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    cleaned
        .rsplit(['.', '(', ')'])
        .find(|part| is_ident(part))
        .map(ToOwned::to_owned)
}

fn is_ident(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_pascal_word(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('A'..='Z')) && chars.all(|ch| ch.is_ascii_alphabetic())
}
