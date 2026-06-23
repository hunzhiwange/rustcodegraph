//! Grammar loading and language detection.
//!
//! The Rust extractor now owns grammar loading through native tree-sitter crates.
//! Some legacy names remain because translated extractor code still expects the
//! old web-tree-sitter-shaped facade.
//!
//! 这里同时承担“文件路径到语言”的轻量判断和“语言到 parser”的缓存。
//! 复杂文件格式如 Vue/Svelte/Astro 由专用抽取器处理，不一定有原生 grammar。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use crate::types::Language;
use crate::web_tree_sitter::{Language as RuntimeLanguage, Parser};

pub type GrammarLanguage = Language;

static PARSER_INITIALIZED: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));
static PARSER_CACHE: LazyLock<Mutex<HashMap<String, Parser>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static LANGUAGE_CACHE: LazyLock<Mutex<HashMap<String, RuntimeLanguage>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static UNAVAILABLE_GRAMMAR_ERRORS: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub const NATIVE_GRAMMAR_REGISTRY: &[(&str, &str)] = &[
    // 第一列是 RustCodeGraph 内部语言 key，第二列是 runtime 侧 grammar id。
    // JSX 复用 javascript grammar，TSX 则有单独 grammar。
    ("typescript", "typescript"),
    ("tsx", "tsx"),
    ("javascript", "javascript"),
    ("jsx", "javascript"),
    ("python", "python"),
    ("go", "go"),
    ("rust", "rust"),
    ("java", "java"),
    ("c", "c"),
    ("cpp", "cpp"),
    ("csharp", "csharp"),
    ("php", "php"),
    ("ruby", "ruby"),
    ("swift", "swift"),
    ("kotlin", "kotlin"),
    ("dart", "dart"),
    ("pascal", "pascal"),
    ("scala", "scala"),
    ("lua", "lua"),
    ("r", "r"),
    ("luau", "luau"),
    ("objc", "objc"),
];

pub const EXTENSION_MAP: &[(&str, &str)] = &[
    // 顺序保持从更具体到更普通的扩展名；detect_language 用精确后缀匹配，
    // 但人读表时仍应把同一语言的变体放在一起。
    (".ts", "typescript"),
    (".tsx", "tsx"),
    (".mts", "typescript"),
    (".cts", "typescript"),
    (".js", "javascript"),
    (".mjs", "javascript"),
    (".cjs", "javascript"),
    (".xsjs", "javascript"),
    (".xsjslib", "javascript"),
    (".jsx", "jsx"),
    (".py", "python"),
    (".pyw", "python"),
    (".go", "go"),
    (".rs", "rust"),
    (".java", "java"),
    (".c", "c"),
    (".h", "c"),
    (".cpp", "cpp"),
    (".cc", "cpp"),
    (".cxx", "cpp"),
    (".hpp", "cpp"),
    (".hxx", "cpp"),
    (".cs", "csharp"),
    (".cshtml", "razor"),
    (".razor", "razor"),
    (".php", "php"),
    (".module", "php"),
    (".install", "php"),
    (".theme", "php"),
    (".inc", "php"),
    (".yml", "yaml"),
    (".yaml", "yaml"),
    (".twig", "twig"),
    (".rb", "ruby"),
    (".rake", "ruby"),
    (".swift", "swift"),
    (".kt", "kotlin"),
    (".kts", "kotlin"),
    (".dart", "dart"),
    (".liquid", "liquid"),
    (".svelte", "svelte"),
    (".vue", "vue"),
    (".astro", "astro"),
    (".r", "r"),
    (".pas", "pascal"),
    (".dpr", "pascal"),
    (".dpk", "pascal"),
    (".lpr", "pascal"),
    (".dfm", "pascal"),
    (".fmx", "pascal"),
    (".scala", "scala"),
    (".sc", "scala"),
    (".lua", "lua"),
    (".luau", "luau"),
    (".m", "objc"),
    (".mm", "objc"),
    (".xml", "xml"),
    (".properties", "properties"),
];

pub fn is_source_file(file_path: &str) -> bool {
    // 部分框架入口没有传统源码扩展名，或是 JSON 但语义上包含模板引用；
    // 先放行这些特殊路径，再走扩展名表。
    if is_play_routes_file(file_path) || is_shopify_liquid_json(file_path) {
        return true;
    }
    let lower = file_path.to_ascii_lowercase();
    EXTENSION_MAP.iter().any(|(ext, _)| lower.ends_with(ext))
}

pub fn is_shopify_liquid_json(file_path: &str) -> bool {
    let lower = file_path.to_ascii_lowercase();
    lower.ends_with(".json") && (lower.contains("/templates/") || lower.starts_with("templates/"))
        || (lower.ends_with(".json")
            && (lower.contains("/sections/") || lower.starts_with("sections/")))
}

pub fn is_play_routes_file(file_path: &str) -> bool {
    file_path == "conf/routes"
        || file_path.ends_with("/conf/routes")
        || file_path.ends_with(".routes")
}

pub async fn init_grammars() -> Result<(), String> {
    let mut initialized = PARSER_INITIALIZED.lock().map_err(|err| err.to_string())?;
    if *initialized {
        return Ok(());
    }
    Parser::init(None)?;
    *initialized = true;
    Ok(())
}

pub async fn load_grammars_for_languages(languages: &[Language]) -> Result<(), String> {
    init_grammars().await?;

    let mut requested: Vec<String> = languages.iter().map(language_key).collect();
    if requested
        .iter()
        .any(|lang| matches!(lang.as_str(), "svelte" | "vue" | "astro"))
    {
        // 单文件组件会把脚本块委托给 TS/JS 抽取器；预加载这两个 grammar
        // 可以避免第一次遇到嵌入脚本时才失败或延迟。
        requested.push("typescript".to_owned());
        requested.push("javascript".to_owned());
    }
    requested.sort();
    requested.dedup();

    let mut language_cache = LANGUAGE_CACHE.lock().map_err(|err| err.to_string())?;
    let mut errors = UNAVAILABLE_GRAMMAR_ERRORS
        .lock()
        .map_err(|err| err.to_string())?;

    for lang in requested {
        if native_grammar_entry(&lang).is_none() || language_cache.contains_key(&lang) {
            continue;
        }
        if errors.contains_key(&lang) {
            // 加载失败也缓存，避免每个文件都重复尝试同一个缺失 grammar。
            continue;
        }

        let grammar_id = grammar_identifier_for_language(&lang);
        match RuntimeLanguage::load(&grammar_id) {
            Ok(language) => {
                language_cache.insert(lang, language);
            }
            Err(message) => {
                errors.insert(lang, message);
            }
        }
    }
    Ok(())
}

pub async fn load_all_grammars() -> Result<(), String> {
    let languages = NATIVE_GRAMMAR_REGISTRY
        .iter()
        .map(|(lang, _)| language_from_key(lang))
        .collect::<Vec<_>>();
    load_grammars_for_languages(&languages).await
}

pub fn is_grammars_initialized() -> bool {
    PARSER_INITIALIZED
        .lock()
        .map(|value| *value)
        .unwrap_or(false)
}

pub fn get_parser(language: Language) -> Option<Parser> {
    let key = language_key(&language);
    if let Ok(cache) = PARSER_CACHE.lock()
        && let Some(parser) = cache.get(&key)
    {
        // Parser 可 clone，缓存命中时给调用方独立实例，避免跨文件 parse
        // 共享可变状态。
        return Some(parser.clone());
    }

    let lang = if let Some(cached) = LANGUAGE_CACHE.lock().ok()?.get(&key).cloned() {
        cached
    } else {
        let loaded = RuntimeLanguage::for_code_language(language).ok()?;
        if let Ok(mut cache) = LANGUAGE_CACHE.lock() {
            cache.insert(key.clone(), loaded.clone());
        }
        loaded
    };
    let mut parser = Parser::default();
    parser.set_language(Some(lang));
    if let Ok(mut cache) = PARSER_CACHE.lock() {
        cache.insert(key, parser.clone());
    }
    Some(parser)
}

pub fn detect_language(file_path: &str, source: Option<&str>) -> Language {
    // Play routes 在类型系统里暂归 YAML，是为了复用轻量文件级路径；
    // 真正的路由语义由 framework resolver 处理。
    if is_play_routes_file(file_path) {
        return Language::Yaml;
    }
    if is_shopify_liquid_json(file_path) {
        return Language::Liquid;
    }

    let ext = extension_with_dot(file_path).unwrap_or_default();
    let mut language = EXTENSION_MAP
        .iter()
        .find_map(|(candidate, lang)| (*candidate == ext).then(|| language_from_key(lang)))
        .unwrap_or(Language::Unknown);

    if matches!(language, Language::C)
        && ext == ".h"
        && let Some(source) = source
    {
        // `.h` 在 C/C++/ObjC 间歧义很大；只扫描前 8KB 寻找高信号关键字，
        // 保持检测便宜，同时覆盖大多数头文件声明区。
        if looks_like_cpp(source) {
            language = Language::Cpp;
        } else if looks_like_objc(source) {
            language = Language::ObjC;
        }
    }

    language
}

fn extension_with_dot(file_path: &str) -> Option<String> {
    Path::new(file_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{}", ext.to_ascii_lowercase()))
}

fn looks_like_cpp(source: &str) -> bool {
    let sample = &source[..source.len().min(8192)];
    [
        "namespace",
        "template<",
        "template <",
        "virtual",
        "using namespace",
    ]
    .iter()
    .any(|needle| sample.contains(needle))
        || sample.contains("class ")
        || sample.contains("public:")
        || sample.contains("private:")
        || sample.contains("protected:")
}

fn looks_like_objc(source: &str) -> bool {
    let sample = &source[..source.len().min(8192)];
    ["@interface", "@implementation", "@protocol", "@synthesize"]
        .iter()
        .any(|needle| sample.contains(needle))
}

pub fn is_language_supported(language: Language) -> bool {
    // “支持”不等于有 native grammar：模板/配置类语言走专用抽取器或文件级节点。
    match language_key(&language).as_str() {
        "svelte" | "vue" | "astro" | "liquid" | "razor" | "yaml" | "twig" | "xml"
        | "properties" => true,
        "unknown" => false,
        key => native_grammar_entry(key).is_some(),
    }
}

pub fn is_grammar_loaded(language: Language) -> bool {
    match language_key(&language).as_str() {
        "svelte" | "vue" | "astro" | "liquid" | "razor" | "yaml" | "twig" | "xml"
        | "properties" => true,
        key => LANGUAGE_CACHE
            .lock()
            .map(|cache| cache.contains_key(key))
            .unwrap_or(false),
    }
}

pub fn is_file_level_only_language(language: Language) -> bool {
    matches!(
        language,
        Language::Yaml | Language::Twig | Language::Properties
    )
}

pub fn get_supported_languages() -> Vec<Language> {
    let mut languages = NATIVE_GRAMMAR_REGISTRY
        .iter()
        .map(|(lang, _)| language_from_key(lang))
        .collect::<Vec<_>>();
    languages.extend([
        Language::Svelte,
        Language::Vue,
        Language::Astro,
        Language::Liquid,
    ]);
    languages
}

pub fn reset_parser(language: Language) {
    let key = language_key(&language);
    if let Ok(mut cache) = PARSER_CACHE.lock() {
        cache.remove(&key);
    }
}

pub fn clear_parser_cache() {
    if let Ok(mut cache) = PARSER_CACHE.lock() {
        cache.clear();
    }
    if let Ok(mut errors) = UNAVAILABLE_GRAMMAR_ERRORS.lock() {
        errors.clear();
    }
}

pub fn get_unavailable_grammar_errors() -> HashMap<String, String> {
    UNAVAILABLE_GRAMMAR_ERRORS
        .lock()
        .map(|errors| errors.clone())
        .unwrap_or_default()
}

pub fn get_language_display_name(language: Language) -> &'static str {
    match language {
        Language::TypeScript => "TypeScript",
        Language::JavaScript => "JavaScript",
        Language::Tsx => "TypeScript (TSX)",
        Language::Jsx => "JavaScript (JSX)",
        Language::Python => "Python",
        Language::Go => "Go",
        Language::Rust => "Rust",
        Language::R => "R",
        Language::Java => "Java",
        Language::C => "C",
        Language::Cpp => "C++",
        Language::CSharp => "C#",
        Language::Razor => "Razor/Blazor",
        Language::Php => "PHP",
        Language::Ruby => "Ruby",
        Language::Swift => "Swift",
        Language::Kotlin => "Kotlin",
        Language::Dart => "Dart",
        Language::Svelte => "Svelte",
        Language::Vue => "Vue",
        Language::Astro => "Astro",
        Language::Liquid => "Liquid",
        Language::Pascal => "Pascal / Delphi",
        Language::Scala => "Scala",
        Language::Lua => "Lua",
        Language::Luau => "Luau",
        Language::ObjC => "Objective-C",
        Language::Yaml => "YAML",
        Language::Twig => "Twig",
        Language::Xml => "XML",
        Language::Properties => "Java properties",
        Language::Unknown => "Unknown",
    }
}

fn native_grammar_entry(language: &str) -> Option<&'static str> {
    NATIVE_GRAMMAR_REGISTRY
        .iter()
        .find_map(|(lang, grammar_id)| (*lang == language).then_some(*grammar_id))
}

fn grammar_identifier_for_language(language: &str) -> String {
    native_grammar_entry(language)
        .unwrap_or_default()
        .to_owned()
}

pub fn language_key(language: &Language) -> String {
    match language {
        Language::TypeScript => "typescript",
        Language::Tsx => "tsx",
        Language::JavaScript => "javascript",
        Language::Jsx => "jsx",
        Language::Python => "python",
        Language::Go => "go",
        Language::Rust => "rust",
        Language::Java => "java",
        Language::C => "c",
        Language::Cpp => "cpp",
        Language::CSharp => "csharp",
        Language::Php => "php",
        Language::Ruby => "ruby",
        Language::Swift => "swift",
        Language::Kotlin => "kotlin",
        Language::Dart => "dart",
        Language::Pascal => "pascal",
        Language::Scala => "scala",
        Language::Lua => "lua",
        Language::R => "r",
        Language::Luau => "luau",
        Language::ObjC => "objc",
        Language::Svelte => "svelte",
        Language::Vue => "vue",
        Language::Astro => "astro",
        Language::Liquid => "liquid",
        Language::Razor => "razor",
        Language::Yaml => "yaml",
        Language::Twig => "twig",
        Language::Xml => "xml",
        Language::Properties => "properties",
        Language::Unknown => "unknown",
    }
    .to_owned()
}

fn language_from_key(language: &str) -> Language {
    match language {
        "typescript" => Language::TypeScript,
        "tsx" => Language::Tsx,
        "javascript" => Language::JavaScript,
        "jsx" => Language::Jsx,
        "python" => Language::Python,
        "go" => Language::Go,
        "rust" => Language::Rust,
        "java" => Language::Java,
        "c" => Language::C,
        "cpp" => Language::Cpp,
        "csharp" => Language::CSharp,
        "php" => Language::Php,
        "ruby" => Language::Ruby,
        "swift" => Language::Swift,
        "kotlin" => Language::Kotlin,
        "dart" => Language::Dart,
        "pascal" => Language::Pascal,
        "scala" => Language::Scala,
        "lua" => Language::Lua,
        "r" => Language::R,
        "luau" => Language::Luau,
        "objc" => Language::ObjC,
        "svelte" => Language::Svelte,
        "vue" => Language::Vue,
        "astro" => Language::Astro,
        "liquid" => Language::Liquid,
        "razor" => Language::Razor,
        "yaml" => Language::Yaml,
        "twig" => Language::Twig,
        "xml" => Language::Xml,
        "properties" => Language::Properties,
        _ => Language::Unknown,
    }
}
