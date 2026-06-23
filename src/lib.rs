//! RustCodeGraph library crate surface.
//!
//! 这个文件只声明模块树并重新导出 `index` facade。公共 API 的行为入口
//! 仍集中在 `src/index.rs`，这里避免放业务逻辑，方便 CLI、MCP 和测试
//! 使用同一套模块可见性。

pub mod add_lang;
pub mod agent_eval;
pub mod directory;
pub mod errors;
pub mod index;
pub mod release;
pub mod types;
pub mod utils;
pub mod web_tree_sitter;

pub mod context {
    pub mod formatter;
    pub mod index;
    pub mod markers;
}

pub mod db {
    pub mod index;
    pub mod migrations;
    pub mod queries;
    pub mod sqlite_adapter;
}

pub mod extraction {
    pub mod astro_extractor;
    pub mod dfm_extractor;
    pub mod extraction_version;
    pub mod function_ref;
    pub mod generated_detection;
    pub mod grammars;
    pub mod index;
    pub mod languages {
        pub mod index;

        pub use index::{
            c_cpp, csharp, dart, go, java, javascript, kotlin, lua, luau, objc, pascal, php,
            python, r, ruby, rust, scala, swift, typescript,
        };
    }
    pub mod liquid_extractor;
    pub mod mybatis_extractor;
    pub mod parse_worker;
    pub mod razor_extractor;
    pub mod svelte_extractor;
    pub mod tree_sitter;
    pub mod tree_sitter_helpers;
    pub mod tree_sitter_types;
    pub mod vue_extractor;
}

pub mod graph {
    pub mod index;
    pub mod queries;
    pub mod traversal;
}

pub mod installer {
    pub mod clack;
    pub mod config_writer;
    pub mod index;
    pub mod instructions_template;
    pub mod targets {
        pub mod antigravity;
        pub mod claude;
        pub mod codex;
        pub mod cursor;
        pub mod gemini;
        pub mod hermes;
        pub mod kiro;
        pub mod opencode;
        pub mod registry;
        pub mod shared;
        pub mod toml;
        pub mod types;
    }
}

pub mod mcp {
    pub mod daemon;
    pub mod daemon_manager;
    pub mod daemon_paths;
    pub mod daemon_registry;
    pub mod dynamic_boundaries;
    pub mod engine;
    pub mod index;
    pub mod liveness_watchdog;
    pub mod ppid_watchdog;
    pub mod proxy;
    pub mod server_instructions;
    pub mod session;
    pub mod stdin_teardown;
    pub mod tools;
    pub mod transport;
    pub mod version;
}

pub mod resolution {
    pub mod callback_synthesizer;
    pub mod frameworks {
        pub mod index;

        pub use index::{
            astro, cargo_workspace, csharp, drupal, expo_modules, express, fabric, go, java,
            laravel, nestjs, play, python, react, react_native, ruby, rust, svelte, swift,
            swift_objc, vue,
        };
    }
    pub mod go_module;
    pub mod import_resolver;
    pub mod index;
    pub mod lru_cache;
    pub mod name_matcher;
    pub mod path_aliases;
    pub mod strip_comments;
    pub mod swift_objc_bridge;
    pub mod types;
    pub mod workspace_packages;
}

pub mod search {
    pub mod query_parser;
    pub mod query_utils;
}

pub mod sync {
    pub mod git_hooks;
    pub mod index;
    pub mod watch_policy;
    pub mod watcher;
    pub mod worktree;
}

pub mod telemetry {
    pub mod index;
}

pub mod ui {
    pub mod glyphs;
    pub mod shimmer_progress;
    pub mod shimmer_worker;
    pub mod types;
}

pub mod upgrade {
    pub mod index;
}

pub use index::*;
