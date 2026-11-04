use rustcodegraph::{CodeGraph, types::NodeKind};
fn check(source: &str, expected: &[&str]) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir = Temp(std::env::temp_dir().join(format!(
        "rcg245-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&dir.0).unwrap();
    std::fs::write(dir.0.join("store.ts"), source).unwrap();
    let mut graph = CodeGraph::init_sync(&dir.0).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let nodes = graph.get_nodes_in_file("store.ts");
    let mut registered: Vec<_> = nodes
        .iter()
        .filter(|n| {
            n.signature
                .as_deref()
                .is_some_and(|s| s.starts_with("= Vue store "))
        })
        .map(|n| {
            assert_eq!(n.kind, NodeKind::Function);
            n.name.as_str()
        })
        .collect();
    if source.contains("return fetchUser()") {
        let action = nodes
            .iter()
            .find(|n| n.name == "login" && n.signature.as_deref() == Some("= Vue store actions"))
            .unwrap();
        let target = nodes.iter().find(|n| n.name == "fetchUser").unwrap();
        assert!(
            graph
                .get_outgoing_edges(&action.id)
                .iter()
                .any(|e| e.target == target.id),
            "registered member must retain body calls"
        );
    }
    registered.sort();
    let mut expected = expected.to_vec();
    expected.sort();
    assert_eq!(registered, expected);
    for node in &nodes {
        assert!(
            graph.get_outgoing_edges(&node.id).iter().all(|e| e
                .metadata
                .as_ref()
                .and_then(|m| m.get("synthesizedBy"))
                .and_then(|v| v.as_str())
                .is_none_or(|s| !matches!(s, "pinia-store" | "vuex-dispatch"))),
            "registration alone must not dispatch"
        );
    }
    assert!(graph.index_all(Default::default()).success);
    assert_eq!(nodes.len(), graph.get_nodes_in_file("store.ts").len());
    graph.close();
}
#[test]
fn case_245_registered_collections() {
    check(
        "import {defineStore as make} from 'pinia'; export const useAccount = make('account', {actions:{login(){return fetchUser();}, logout:()=>clearUser()}, getters:{name: s=>s.name}, mutations:{SET: function(s){s.name='x'}}}); function fetchUser(){} function clearUser(){}",
        &["login", "logout", "name", "SET"],
    );
    check(
        "import {createStore} from 'vuex'; const actions={load(){return 1}}; export const store=createStore({actions, mutations:{save(s){return 2}},getters:{count:s=>s.n}});",
        &["load", "save", "count"],
    );
}
#[test]
fn case_245_vuex_default_module() {
    check(
        "const actions={login(){return saveToken()}}; const mutations={TOKEN:(s,t)=>{s.token=t}}; function saveToken(){} export default {namespaced:true, actions, mutations, getters:{token:s=>s.token}};",
        &["login", "TOKEN", "token"],
    );
}
#[test]
fn case_245_missing_registration() {
    check(
        "const actions={login(){}}; const getters={name:s=>s.name}; export function login(){}",
        &[],
    );
    check(
        "import {defineStore} from './fake'; export const store=defineStore('x',{actions:{login(){}}});",
        &[],
    );
}
#[test]
fn case_245_dynamic_and_unrelated_members() {
    check(
        "import {defineStore} from 'pinia'; function unrelated(defineStore){return defineStore('x',{actions:{login(){}}})}",
        &[],
    );
    check(
        "import {defineStore} from 'pinia'; const key='login'; export const store=defineStore('x',{actions:{[key](){}, value:42}, helper(){}, state:()=>({})});",
        &[],
    );
}
