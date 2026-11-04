use rustcodegraph::{CodeGraph, mcp::tools::ToolHandler, types::EdgeProvenance};
use serde_json::json;
struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = Self(std::env::temp_dir().join(format!(
            "rcg246-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )));
        std::fs::create_dir_all(&p.0).unwrap();
        p
    }
    fn write(&self, name: &str, source: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn pairs(g: &mut CodeGraph, file: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for n in g.get_nodes_in_file(file) {
        for e in g.get_outgoing_edges(&n.id) {
            if e.metadata
                .as_ref()
                .and_then(|m| m.get("synthesizedBy"))
                .and_then(|v| v.as_str())
                .is_some_and(|s| matches!(s, "pinia-store" | "vuex-dispatch"))
            {
                assert_eq!(e.provenance, Some(EdgeProvenance::Heuristic));
                assert!(
                    e.metadata.as_ref().unwrap()["registeredAt"]
                        .as_str()
                        .unwrap()
                        .starts_with(&format!("{file}:"))
                );
                pairs.push((n.name.clone(), e.target));
            }
        }
    }
    pairs.sort();
    pairs
}
fn flow(g: &CodeGraph, query: &str, symbols: &[&str]) {
    let mut h = ToolHandler::new(true);
    h.set_default_code_graph(g);
    let args = serde_json::from_value(json!({"query":query})).unwrap();
    let r = serde_json::to_value(h.execute("rustcodegraph_explore", &args)).unwrap();
    let text = r["content"]
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
        text.lines().any(|l| l.starts_with("- Entry (")
            && symbols.iter().all(|s| l.contains(s))
            && l.contains("registeredAt")),
        "{text}"
    );
    h.close_all();
}
#[test]
fn case_246_pinia_flow_and_removal() {
    let p = Project::new();
    let store = "import {defineStore as make} from 'pinia';\nexport const useAccount=make('account',{actions:{\n login(){return finish();}\n}});\nfunction finish(){}";
    p.write("store.ts", store);
    p.write("other/store.ts", &store.replace("finish", "wrongEnd"));
    p.write("entry.ts","import {useAccount as useSelected} from './store';\nexport function Entry(){const account=useSelected(); return account.login();}");
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(pairs(&mut g, "entry.ts").len(), 1);
    flow(
        &g,
        "Entry login finish",
        &["login (store.ts:", "finish (store.ts:", "pinia-store"],
    );
    let count = g.get_nodes_in_file("store.ts").len();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(count, g.get_nodes_in_file("store.ts").len());
    p.write("store.ts", &store.replace("from 'pinia'", "from './fake'"));
    g.sync(Default::default());
    assert!(pairs(&mut g, "entry.ts").is_empty());
    g.close();
}
#[test]
fn case_246_vuex_flow_and_registration_removal() {
    let p = Project::new();
    p.write("account.ts","const actions={\n login({commit}){commit('SAVE');}\n};\nconst mutations={\n SAVE(){finish();}\n};\nexport default {namespaced:true,actions,mutations};\nfunction finish(){}");
    p.write(
        "other/account.ts",
        "export default {namespaced:true, actions:{login(){}}, mutations:{SAVE(){}}};",
    );
    let store = "import {createStore} from 'vuex';\nimport account from './account';\nexport const store=createStore({modules:{user:account}});";
    p.write("store.ts", store);
    p.write(
        "entry.ts",
        "import {store} from './store';\nexport function Entry(){store.dispatch('user/login');}",
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(pairs(&mut g, "entry.ts").len(), 1);
    assert_eq!(pairs(&mut g, "account.ts").len(), 1);
    flow(
        &g,
        "Entry login SAVE finish",
        &[
            "login (account.ts:",
            "SAVE (account.ts:",
            "finish (account.ts:",
            "vuex-dispatch",
        ],
    );
    p.write("store.ts", &store.replace("user:account", ""));
    g.sync(Default::default());
    assert!(pairs(&mut g, "entry.ts").is_empty());
    assert!(pairs(&mut g, "account.ts").is_empty());
    g.close();
}
#[test]
fn case_246_unregistered_dynamic_shadowed_and_cross_app() {
    let p = Project::new();
    p.write("store.ts","import {defineStore} from 'pinia';\nexport const useAccount=defineStore('x',{actions:{login(){}}});");
    p.write(
        "vx.ts",
        "import {createStore} from 'vuex';\nexport const store=createStore({actions:{login(){}}});",
    );
    p.write("entry.ts","import {useAccount} from './store';\nimport {store} from './vx';\nfunction dynamic(key){store.dispatch(key); const a=useAccount(); a[key]();}\nfunction shadow(useAccount){const a=useAccount();a.login();}\nfunction unrelated(store){store.dispatch('login');}\nfunction missing(){dispatch('login');}\nfunction rebound(){let a=useAccount();a={login(){}};a.login();}\nconst globalAccount=useAccount();\nfunction param(globalAccount){globalAccount.login();}");
    p.write(
        "other/entry.ts",
        "function Entry(){const a=useAccount();a.login(); store.dispatch('login');}",
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert!(pairs(&mut g, "entry.ts").is_empty());
    assert!(pairs(&mut g, "other/entry.ts").is_empty());
    g.close();
}

#[test]
fn case_246_exact_store_identity_and_root_vuex() {
    let p = Project::new();
    p.write("stores.ts", "import {defineStore} from 'pinia';\nexport const useOne=defineStore('one',{actions:{\n login(){finishOne();}\n}});\nexport const useTwo=defineStore('two',{actions:{\n login(){finishTwo();}\n}});\nfunction finishOne(){}\nfunction finishTwo(){}");
    p.write("vx.ts", "import {createStore} from 'vuex';\nexport const store=createStore({actions:{\n login({commit}){commit('SAVE');}\n},mutations:{\n SAVE(){finish();}\n}});\nfunction finish(){}");
    p.write("entry.ts", "import {useOne} from './stores';\nimport {store as selected} from './vx';\nexport function Entry(){const a=useOne();a.login();}\nexport function Root(){selected.dispatch('login');}\nexport function Mutation(){selected.commit('SAVE');}");
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let edges = pairs(&mut g, "entry.ts");
    assert_eq!(edges.len(), 3);
    let nodes = g.get_nodes_in_file("stores.ts");
    let target = nodes
        .iter()
        .find(|n| n.name == "login" && n.start_line == 3)
        .unwrap();
    assert!(edges.contains(&("Entry".into(), target.id.clone())));
    assert_eq!(pairs(&mut g, "vx.ts").len(), 1);
    g.close();
}
