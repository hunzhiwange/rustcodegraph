//! Framework resolver registry translated from `frameworks/index.ts`.
//!
//! 这里是 framework resolver 的统一注册表。核心解析器只依赖 trait object，
//! 具体框架按 detect/language 过滤后参与 extract/resolve。

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, OnceLock, RwLock};

use crate::resolution::types::{FrameworkResolver, ResolutionContext};
use crate::types::Language;

#[path = "astro.rs"]
pub mod astro;
#[path = "cargo_workspace.rs"]
pub mod cargo_workspace;
#[path = "csharp.rs"]
pub mod csharp;
#[path = "drupal.rs"]
pub mod drupal;
#[path = "expo_modules.rs"]
pub mod expo_modules;
#[path = "express.rs"]
pub mod express;
#[path = "fabric.rs"]
pub mod fabric;
#[path = "go.rs"]
pub mod go;
#[path = "java.rs"]
pub mod java;
#[path = "laravel.rs"]
pub mod laravel;
#[path = "nestjs.rs"]
pub mod nestjs;
#[path = "play.rs"]
pub mod play;
#[path = "python.rs"]
pub mod python;
#[path = "react.rs"]
pub mod react;
#[path = "react_native.rs"]
pub mod react_native;
#[path = "ruby.rs"]
pub mod ruby;
#[path = "rust.rs"]
pub mod rust;
#[path = "svelte.rs"]
pub mod svelte;
#[path = "swift.rs"]
pub mod swift;
#[path = "swift_objc.rs"]
pub mod swift_objc;
#[path = "vue.rs"]
pub mod vue;

pub type ResolverRef = Arc<dyn FrameworkResolver>;

fn default_resolvers() -> Vec<ResolverRef> {
    // 顺序大体按生态分组；新增 resolver 时同时考虑 detect 成本和同语言 resolver
    // 之间的重叠程度。
    vec![
        // PHP
        Arc::new(laravel::LaravelResolver),
        Arc::new(drupal::DrupalResolver),
        // JavaScript/TypeScript
        Arc::new(express::ExpressResolver),
        Arc::new(nestjs::NestJsResolver),
        Arc::new(react::ReactResolver),
        Arc::new(svelte::SvelteResolver),
        Arc::new(vue::VueResolver),
        Arc::new(astro::AstroResolver),
        // Python
        Arc::new(python::DjangoResolver),
        Arc::new(python::FlaskResolver),
        Arc::new(python::FastApiResolver),
        // Ruby
        Arc::new(ruby::RailsResolver),
        // Java / Play
        Arc::new(java::SpringResolver),
        Arc::new(play::PlayResolver),
        // Go
        Arc::new(go::GoResolver),
        // Rust
        Arc::new(rust::RustResolver),
        // C#
        Arc::new(csharp::AspNetResolver),
        // Swift
        Arc::new(swift::SwiftUiResolver),
        Arc::new(swift::UIKitResolver),
        Arc::new(swift::VaporResolver),
        // Cross-language and RN bridges
        Arc::new(swift_objc::SwiftObjcBridgeResolver),
        Arc::new(react_native::ReactNativeBridgeResolver),
        Arc::new(expo_modules::ExpoModulesResolver),
        Arc::new(fabric::FabricViewResolver),
    ]
}

fn registry() -> &'static RwLock<Vec<ResolverRef>> {
    // 测试可以动态注册 resolver；全局 RwLock 保持读多写少的使用模式简单。
    static REGISTRY: OnceLock<RwLock<Vec<ResolverRef>>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(default_resolvers()))
}

pub fn get_all_framework_resolvers() -> Vec<ResolverRef> {
    registry()
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

pub fn get_framework_resolver(name: &str) -> Option<ResolverRef> {
    registry().read().ok().and_then(|guard| {
        guard
            .iter()
            .find(|resolver| resolver.name() == name)
            .cloned()
    })
}

pub fn detect_frameworks(context: &mut dyn ResolutionContext) -> Vec<ResolverRef> {
    let mut detected = Vec::new();
    for resolver in get_all_framework_resolvers() {
        // 单个 resolver 的检测 bug 不能让整个索引失败；跳过 panic 的 resolver。
        if catch_unwind(AssertUnwindSafe(|| resolver.detect(context))).unwrap_or(false) {
            detected.push(resolver);
        }
    }
    detected
}

pub fn get_applicable_frameworks(detected: &[ResolverRef], language: Language) -> Vec<ResolverRef> {
    // 没声明 languages 的 resolver 视为跨语言 resolver，例如某些桥接/配置解析。
    detected
        .iter()
        .filter(|resolver| {
            resolver
                .languages()
                .map(|languages| languages.contains(&language))
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

pub fn register_framework_resolver(resolver: ResolverRef) {
    // 同名注册覆盖旧 resolver，方便测试替换和实验性扩展。
    let Ok(mut guard) = registry().write() else {
        return;
    };
    if let Some(index) = guard
        .iter()
        .position(|existing| existing.name() == resolver.name())
    {
        guard.remove(index);
    }
    guard.push(resolver);
}

pub use astro::{ASTRO_RESOLVER, AstroResolver};
pub use csharp::{ASPNET_RESOLVER, AspNetResolver};
pub use drupal::{DRUPAL_RESOLVER, DrupalResolver};
pub use expo_modules::{EXPO_MODULES_RESOLVER, ExpoModulesResolver};
pub use express::{EXPRESS_RESOLVER, ExpressResolver};
pub use fabric::{FABRIC_VIEW_RESOLVER, FabricViewResolver};
pub use go::{GO_RESOLVER, GoResolver};
pub use java::{SPRING_RESOLVER, SpringResolver};
pub use laravel::{FACADE_MAPPINGS, LARAVEL_RESOLVER, LaravelResolver};
pub use nestjs::{NESTJS_RESOLVER, NestJsResolver};
pub use play::{PLAY_RESOLVER, PlayResolver};
pub use python::{
    DJANGO_RESOLVER, DjangoResolver, FASTAPI_RESOLVER, FLASK_RESOLVER, FastApiResolver,
    FlaskResolver,
};
pub use react::{REACT_RESOLVER, ReactResolver};
pub use react_native::{REACT_NATIVE_BRIDGE_RESOLVER, ReactNativeBridgeResolver};
pub use ruby::{RAILS_RESOLVER, RailsResolver};
pub use rust::{RUST_RESOLVER, RustResolver};
pub use svelte::{SVELTE_RESOLVER, SvelteResolver};
pub use swift::{
    SWIFTUI_RESOLVER, SwiftUiResolver, UIKIT_RESOLVER, UIKitResolver, VAPOR_RESOLVER, VaporResolver,
};
pub use swift_objc::{SWIFT_OBJC_BRIDGE_RESOLVER, SwiftObjcBridgeResolver};
pub use vue::{VUE_RESOLVER, VueResolver};
