//! Extraction orchestrator facade.
//!
//! Public callers continue to import from `crate::extraction::index`; the
//! implementation is split into focused submodules below.
//!
//! 这是抽取层的稳定 re-export 门面：外部 API 和旧调用点不需要知道
//! orchestrator、ignore、discovery 等内部拆分。

mod constants;
mod discovery;
mod helpers;
mod ignore;
mod orchestrator;
mod results;

pub use self::discovery::{scan_directory, scan_directory_async};
pub use self::helpers::hash_content;
pub use self::ignore::{
    DefaultIgnore, ScopeIgnore, build_default_ignore, build_scope_ignore,
    discover_embedded_repo_roots,
};
pub use self::orchestrator::ExtractionOrchestrator;
pub use self::results::{GitChanges, IndexPhase, IndexProgress, IndexResult, SyncResult};

pub use crate::extraction::astro_extractor::AstroExtractor;
pub use crate::extraction::dfm_extractor::DfmExtractor;
pub use crate::extraction::grammars::{
    detect_language, get_supported_languages, init_grammars, is_grammar_loaded,
    is_language_supported, load_all_grammars, load_grammars_for_languages,
};
pub use crate::extraction::liquid_extractor::LiquidExtractor;
pub use crate::extraction::mybatis_extractor::MyBatisExtractor;
pub use crate::extraction::razor_extractor::RazorExtractor;
pub use crate::extraction::svelte_extractor::SvelteExtractor;
pub use crate::extraction::tree_sitter::extract_from_source;
pub use crate::extraction::vue_extractor::VueExtractor;
