use rustcodegraph::{CodeGraph, mcp::tools::ToolHandler};
use serde_json::json;
struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "rcg233-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn write(&self, name: &str, source: &str) {
        std::fs::write(self.0.join(name), source).unwrap();
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
const API: &str = r#"import { createApi as makeApi } from '@reduxjs/toolkit/query/react';
export function fetchItems() { return []; }
export const api = makeApi({ endpoints: build => ({
 getItems: build.query({ queryFn: async () => { return {data: fetchItems()}; } }),
 updateItems: build.mutation({ query: body => ({url:'items',body}) })
}) });
export const { useGetItemsQuery, useLazyGetItemsQuery: useDeferred, useUpdateItemsMutation } = api;
"#;
fn synth(graph: &mut CodeGraph, file: &str) -> Vec<(String, String)> {
    let nodes = graph.get_nodes_in_file(file);
    let mut pairs = Vec::new();
    for n in &nodes {
        for e in graph.get_outgoing_edges(&n.id) {
            if e.metadata
                .as_ref()
                .and_then(|m| m.get("synthesizedBy"))
                .and_then(|v| v.as_str())
                == Some("rtk-query")
            {
                assert_eq!(
                    e.provenance,
                    Some(rustcodegraph::types::EdgeProvenance::Heuristic)
                );
                assert!(
                    e.metadata.as_ref().unwrap()["registeredAt"]
                        .as_str()
                        .unwrap()
                        .starts_with(&format!("{file}:"))
                );
                let t = nodes
                    .iter()
                    .find(|t| t.id == e.target)
                    .expect("same file target");
                pairs.push((n.name.clone(), t.name.clone()));
            }
        }
    }
    pairs.sort();
    pairs
}
#[test]
fn case_233_full_flow_and_incremental_removal() {
    let p = Project::new();
    p.write("api.ts", API);
    p.write("view.ts","import {useGetItemsQuery} from './api';\nexport function View(){return useGetItemsQuery();}");
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let pairs = synth(&mut g, "api.ts");
    assert_eq!(pairs.len(), 3, "{pairs:?}");
    let mut h = ToolHandler::new(true);
    h.set_default_code_graph(&g);
    let args = serde_json::from_value(json!({"query":"View useGetItemsQuery getItems fetchItems"}))
        .unwrap();
    let result = serde_json::to_value(h.execute("rustcodegraph_explore", &args)).unwrap();
    let text = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("## Flow (call path among the symbols you queried)"),
        "{text}"
    );
    assert!(
        text.lines().any(|line| line.starts_with("- View (")
            && [
                "useGetItemsQuery (",
                "getItems (",
                "fetchItems (",
                "rtk-query",
                "registeredAt"
            ]
            .iter()
            .all(|symbol| line.contains(symbol))),
        "complete flow missing: {text}"
    );
    assert!(text.contains("rtk-query"), "{text}");
    assert!(text.contains("fetchItems"));
    assert!(text.contains("registeredAt"));
    h.close_all();
    let nodes = g.get_nodes_in_file("api.ts");
    let endpoint = nodes.iter().find(|n| n.name == "getItems").unwrap();
    let end = nodes.iter().find(|n| n.name == "fetchItems").unwrap();
    assert!(
        g.get_outgoing_edges(&endpoint.id)
            .iter()
            .any(|e| e.target == end.id),
        "endpoint must reach its callback's fetchItems"
    );
    assert!(g.index_all(Default::default()).success);
    assert_eq!(pairs, synth(&mut g, "api.ts"));
    assert_eq!(nodes.len(), g.get_nodes_in_file("api.ts").len());
    p.write("api.ts",&API.replace("export const { useGetItemsQuery, useLazyGetItemsQuery: useDeferred, useUpdateItemsMutation } = api;",""));
    g.sync(Default::default());
    assert!(synth(&mut g, "api.ts").is_empty());
    g.close();
}
#[test]
fn case_233_missing_registration_and_dynamic_keys() {
    for source in [
        API.replace("@reduxjs/toolkit/query/react", "./fake"),
        API.replace("getItems:", "[dynamic]:"),
        API.replace("} = api;", "} = unknown;"),
        String::from(
            "export function getItems(){return 1;}\nexport const useGetItemsQuery=()=>getItems();",
        ),
    ] {
        let p = Project::new();
        p.write("api.ts", &source);
        let mut g = CodeGraph::init_sync(&p.0).unwrap();
        assert!(g.index_all(Default::default()).success);
        let pairs = synth(&mut g, "api.ts");
        assert!(
            !pairs.iter().any(|(s, _)| s == "useGetItemsQuery"),
            "{pairs:?}"
        );
        g.close();
    }
}
#[test]
fn case_233_same_name_apis_stay_scoped() {
    let p = Project::new();
    p.write("api.ts", API);
    p.write("other.ts", &API.replace("fetchItems", "otherFetch"));
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(synth(&mut g, "api.ts").len(), 3);
    assert_eq!(synth(&mut g, "other.ts").len(), 3);
    g.close();
}

#[test]
fn case_233_method_and_same_file_api_identity() {
    let p = Project::new();
    p.write(
        "api.ts",
        r#"import {createApi} from '@reduxjs/toolkit/query/react';
export function firstFetch(){return 1;}
export function secondFetch(){return 2;}
export const first = createApi({endpoints(builder){return {
 getItems: builder.query({queryFn(){return firstFetch();}})
};}});
export const second = createApi({endpoints: builder => ({
 getItems: builder.query({queryFn: () => secondFetch()})
})});
export const {useGetItemsQuery: firstHook} = first;
export const {useGetItemsQuery: secondHook} = second;
"#,
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(synth(&mut g, "api.ts").len(), 2);
    let nodes = g.get_nodes_in_file("api.ts");
    for (hook, fetch) in [("firstHook", "firstFetch"), ("secondHook", "secondFetch")] {
        let source = nodes.iter().find(|n| n.name == hook).unwrap();
        let bridge = g
            .get_outgoing_edges(&source.id)
            .into_iter()
            .find(|e| {
                e.metadata
                    .as_ref()
                    .and_then(|m| m.get("synthesizedBy"))
                    .and_then(|v| v.as_str())
                    == Some("rtk-query")
            })
            .unwrap();
        let target = nodes.iter().find(|n| n.name == fetch).unwrap();
        assert!(
            g.get_outgoing_edges(&bridge.target)
                .iter()
                .any(|e| e.target == target.id)
        );
    }
    p.write(
        "api.ts",
        "export const {useGetItemsQuery: firstHook} = first;",
    );
    g.sync(Default::default());
    assert!(synth(&mut g, "api.ts").is_empty());
    g.close();
}
#[test]
fn case_233_uncertain_registration_stays_silent() {
    for source in [
        API.replace("const api =", "let api =") + "\napi = otherApi;",
        API.replace("getItems:", "...otherEndpoints, getItems:"),
        API.replace("endpoints: build", "...options, endpoints: build"),
        API.to_string() + "\napi.useGetItemsQuery = custom;",
    ] {
        let p = Project::new();
        p.write("api.ts", &source);
        let mut g = CodeGraph::init_sync(&p.0).unwrap();
        assert!(g.index_all(Default::default()).success);
        assert!(synth(&mut g, "api.ts").is_empty());
        g.close();
    }
}

#[test]
fn case_233_js_and_tsx_imported_callers() {
    for extension in ["js", "jsx", "tsx"] {
        let p = Project::new();
        let file = format!("api.{extension}");
        p.write(&file, API);
        p.write("view.tsx", "import {useGetItemsQuery} from './api';\nexport function View(){return useGetItemsQuery();}");
        let mut g = CodeGraph::init_sync(&p.0).unwrap();
        assert!(g.index_all(Default::default()).success);
        assert_eq!(synth(&mut g, &file).len(), 3);
        let hook = g
            .get_nodes_in_file(&file)
            .into_iter()
            .find(|n| n.name == "useGetItemsQuery")
            .unwrap();
        let view = g
            .get_nodes_in_file("view.tsx")
            .into_iter()
            .find(|n| n.name == "View")
            .unwrap();
        assert!(
            g.get_outgoing_edges(&view.id)
                .iter()
                .any(|e| e.target == hook.id)
        );
        g.close();
    }
}
