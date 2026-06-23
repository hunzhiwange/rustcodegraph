//! Callback / dynamic-dispatch edge synthesis.
//!
//! This is the Rust counterpart of `callback-synthesizer.ts`. It runs after
//! normal reference resolution and adds reachability edges for runtime dispatch
//! mechanisms that tree-sitter cannot see statically.
//!
//! 合成边的目标不是“猜完整运行时”，而是补足 agent 最常追问的端到端 flow。
//! 每条启发式边都带 provenance/metadata，方便 explore/node 在输出里说明来源。

use std::collections::HashSet;

use crate::db::queries::QueryBuilder;

use super::types::ResolutionContext;

mod callbacks;
mod common;
mod gin;
mod go;
mod go_grpc;
mod java_xml;
mod mobile;
mod overrides;
mod react;
mod templates;

use callbacks::{closure_collection_edges, event_emitter_edges, field_channel_edges};
use gin::gin_middleware_chain_edges;
use go::{go_cross_file_method_contains_edges, go_implements_edges, go_interface_assignment_edges};
use go_grpc::go_grpc_stub_impl_edges;
use java_xml::mybatis_java_xml_edges;
use mobile::{
    expo_cross_platform_edges, fabric_native_impl_edges, rn_cross_platform_edges, rn_event_edges,
};
use overrides::{
    cpp_override_edges, flutter_build_edges, interface_override_edges, kotlin_expect_actual_edges,
};
use react::{react_jsx_child_edges, react_render_edges};
use templates::{pascal_form_edges, svelte_kit_load_edges, vue_template_edges};

/// Synthesize dispatcher-to-callback/dynamic edges and persist them.
pub fn synthesize_callback_edges(
    queries: &mut QueryBuilder,
    ctx: &mut dyn ResolutionContext,
) -> usize {
    // 这两类 Go 边是结构修补：抽取器可能因为 receiver/type 分文件而漏 contains，
    // 或缺少显式 implements。它们先写入 DB，供后续启发式 pass 复用。
    let go_method_contains = go_cross_file_method_contains_edges(queries);
    if !go_method_contains.is_empty() {
        let _ = queries.insert_edges(&go_method_contains);
    }

    let go_impl = go_implements_edges(queries);
    if !go_impl.is_empty() {
        let _ = queries.insert_edges(&go_impl);
    }

    let mut all = Vec::new();
    // 后续 pass 都是运行时/框架调度的保守桥接，统一批量去重后写入。
    all.extend(field_channel_edges(queries, ctx));
    all.extend(closure_collection_edges(queries, ctx));
    all.extend(event_emitter_edges(ctx));
    all.extend(react_render_edges(queries, ctx));
    all.extend(react_jsx_child_edges(ctx));
    all.extend(vue_template_edges(ctx));
    all.extend(svelte_kit_load_edges(ctx));
    all.extend(pascal_form_edges(ctx));
    all.extend(go_interface_assignment_edges(ctx));
    all.extend(flutter_build_edges(queries, ctx));
    all.extend(cpp_override_edges(queries));
    all.extend(interface_override_edges(queries));
    all.extend(kotlin_expect_actual_edges(queries));
    all.extend(rn_event_edges(ctx));
    all.extend(fabric_native_impl_edges(ctx));
    all.extend(expo_cross_platform_edges(queries));
    all.extend(rn_cross_platform_edges(queries));
    all.extend(mybatis_java_xml_edges(queries));
    all.extend(go_grpc_stub_impl_edges(queries));
    all.extend(gin_middleware_chain_edges(queries, ctx));

    let mut merged = Vec::new();
    let mut seen = HashSet::new();
    for edge in all {
        // 多个 pass 可能推导出同一 source/target，保留一条即可，避免 explore path
        // 因重复边膨胀。
        let key = format!("{}>{}", edge.source, edge.target);
        if seen.insert(key) {
            merged.push(edge);
        }
    }
    if !merged.is_empty() {
        let _ = queries.insert_edges(&merged);
    }
    merged.len() + go_impl.len() + go_method_contains.len()
}
