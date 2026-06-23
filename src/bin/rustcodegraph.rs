//! Rust CLI entry point.
//!
//! 真正的 CLI 实现放在 `src/bin/rustcodegraph/` 子模块树里；这里保持极薄入口，
//! 让 cargo binary 名称和模块组织可以独立演进。

#[path = "rustcodegraph/mod.rs"]
mod rustcodegraph_cli;

fn main() {
    rustcodegraph_cli::main();
}
