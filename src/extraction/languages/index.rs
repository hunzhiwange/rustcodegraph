//! Rust translation of `src/extraction/languages/index.ts`.
//!
//! This module keeps registry coverage identical to the TypeScript `EXTRACTORS`
//! map. It uses string language tokens until the Rust `Language` enum from the
//! core-types task is available.

#[path = "c_cpp.rs"]
pub mod c_cpp;
#[path = "csharp.rs"]
pub mod csharp;
#[path = "dart.rs"]
pub mod dart;
#[path = "go.rs"]
pub mod go;
#[path = "java.rs"]
pub mod java;
#[path = "javascript.rs"]
pub mod javascript;
#[path = "kotlin.rs"]
pub mod kotlin;
#[path = "lua.rs"]
pub mod lua;
#[path = "luau.rs"]
pub mod luau;
#[path = "objc.rs"]
pub mod objc;
#[path = "pascal.rs"]
pub mod pascal;
#[path = "php.rs"]
pub mod php;
#[path = "python.rs"]
pub mod python;
#[path = "r.rs"]
pub mod r;
#[path = "ruby.rs"]
pub mod ruby;
#[path = "rust.rs"]
pub mod rust;
#[path = "scala.rs"]
pub mod scala;
#[path = "swift.rs"]
pub mod swift;
#[path = "typescript.rs"]
pub mod typescript;

use crate::extraction::tree_sitter_types::LanguageExtractor;

pub use c_cpp::{
    C_EXTRACTOR, CPP_EXTRACTOR, c_extractor, cpp_extractor, normalize_cpp_return_type,
};
pub use csharp::{CSHARP_EXTRACTOR, blank_csharp_preprocessor_directives, csharp_extractor};
pub use dart::{DART_EXTRACTOR, dart_extractor};
pub use go::{GO_EXTRACTOR, go_extractor};
pub use java::{JAVA_EXTRACTOR, java_extractor};
pub use javascript::{JAVASCRIPT_EXTRACTOR, javascript_extractor};
pub use kotlin::{KOTLIN_EXTRACTOR, kotlin_extractor};
pub use lua::{LUA_EXTRACTOR, lua_extractor};
pub use luau::{LUAU_EXTRACTOR, luau_extractor};
pub use objc::{OBJC_EXTRACTOR, objc_extractor};
pub use pascal::{PASCAL_EXTRACTOR, pascal_extractor};
pub use php::{PHP_EXTRACTOR, php_extractor};
pub use python::{PYTHON_EXTRACTOR, python_extractor};
pub use r::{R_EXTRACTOR, r_extractor};
pub use ruby::{RUBY_EXTRACTOR, ruby_extractor};
pub use rust::{RUST_EXTRACTOR, rust_extractor};
pub use scala::{SCALA_EXTRACTOR, scala_extractor};
pub use swift::{SWIFT_EXTRACTOR, swift_extractor};
pub use typescript::{TYPESCRIPT_EXTRACTOR, classify_ts_class_member, typescript_extractor};

// 这个列表是 grammar 检测到语言后能进入通用 tree-sitter 抽取器的白名单；
// 新语言需要同时注册模块、常量导出和 extractor_for 分支。
pub const EXTRACTOR_LANGUAGES: &[&str] = &[
    "typescript",
    "tsx",
    "javascript",
    "jsx",
    "python",
    "go",
    "rust",
    "java",
    "c",
    "cpp",
    "csharp",
    "php",
    "ruby",
    "swift",
    "kotlin",
    "dart",
    "pascal",
    "scala",
    "lua",
    "r",
    "luau",
    "objc",
];

pub fn extractor_for(language: &str) -> Option<&'static dyn LanguageExtractor> {
    // TSX/JSX 复用 TS/JS 抽取器，框架层再根据文件内容补组件和路由语义。
    match language {
        "typescript" | "tsx" => Some(typescript_extractor()),
        "javascript" | "jsx" => Some(javascript_extractor()),
        "python" => Some(python_extractor()),
        "go" => Some(go_extractor()),
        "rust" => Some(rust_extractor()),
        "java" => Some(java_extractor()),
        "c" => Some(c_extractor()),
        "cpp" => Some(cpp_extractor()),
        "csharp" => Some(csharp_extractor()),
        "php" => Some(php_extractor()),
        "ruby" => Some(ruby_extractor()),
        "swift" => Some(swift_extractor()),
        "kotlin" => Some(kotlin_extractor()),
        "dart" => Some(dart_extractor()),
        "pascal" => Some(pascal_extractor()),
        "scala" => Some(scala_extractor()),
        "lua" => Some(lua_extractor()),
        "r" => Some(r_extractor()),
        "luau" => Some(luau_extractor()),
        "objc" => Some(objc_extractor()),
        _ => None,
    }
}
