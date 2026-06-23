//! Object-literal method extraction (general AST rule).
//!
//! Rust port of `__tests__/object-literal-methods.test.ts`.
//!
//! The TypeScript source initializes and loads tree-sitter grammars in
//! `beforeAll`. The Rust port keeps these parity cases active so object-valued
//! store actions remain visible to extraction and caller lookup.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::extraction::index::extract_from_source;
use rustcodegraph::types::{ExtractionResult, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};

fn function_names(result: &ExtractionResult) -> Vec<String> {
    result
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Function)
        .map(|node| node.name.clone())
        .collect()
}

fn node_names(result: &ExtractionResult) -> Vec<String> {
    result.nodes.iter().map(|node| node.name.clone()).collect()
}

fn assert_contains(actual: &[String], expected: &str) {
    assert!(
        actual.iter().any(|name| name == expected),
        "expected {actual:?} to contain {expected:?}"
    );
}

fn assert_not_contains(actual: &[String], unexpected: &str) {
    assert!(
        !actual.iter().any(|name| name == unexpected),
        "expected {actual:?} not to contain {unexpected:?}"
    );
}

struct TempProject {
    path: PathBuf,
}

impl TempProject {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).unwrap_or_else(|err| {
            panic!("failed to create temp project {}: {err}", path.display())
        });
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative_path: &str, content: &str) {
        fs::write(self.path.join(relative_path), content)
            .unwrap_or_else(|err| panic!("failed to write fixture {relative_path}: {err}"));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

mod object_literal_method_extraction {
    use super::*;

    #[test]
    fn extracts_zustand_store_actions_object_returned_by_create_as_function_nodes() {
        let code = r#"
      import { create } from 'zustand'
      interface Store {
        count: number
        fetchUser(): Promise<void>
        switchOrganization(id: string): Promise<void>
        reset(): void
      }
      export const useStore = create<Store>((set, get) => ({
        count: 0,
        fetchUser: async () => { await get().reset() },
        switchOrganization: async (id: string) => { set({ count: 1 }) },
        reset: () => set({ count: 0 }),
      }))
    "#;
        let result = extract_from_source("store.ts", code, None, None);
        let fn_names = function_names(&result);
        assert_contains(&fn_names, "fetchUser");
        assert_contains(&fn_names, "switchOrganization");
        assert_contains(&fn_names, "reset");

        // Each action's body was walked: fetchUser references its sibling
        // `reset`, so an in-store calls edge will resolve once the pipeline runs.
        let fetch_user = result
            .nodes
            .iter()
            .find(|node| node.name == "fetchUser")
            .expect("fetchUser should be extracted");
        let fetch_user_refs = result
            .unresolved_references
            .iter()
            .filter(|reference| reference.from_node_id == fetch_user.id)
            .map(|reference| reference.reference_name.clone())
            .collect::<Vec<_>>();
        assert_contains(&fetch_user_refs, "reset");

        // The action's body wasn't mis-attributed to the file scope (the reason
        // we skip the generic body-visit for the store-factory call).
        let file_node = result
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::File)
            .expect("file node should be extracted");
        let file_refs = result
            .unresolved_references
            .iter()
            .filter(|reference| reference.from_node_id == file_node.id)
            .map(|reference| reference.reference_name.clone())
            .collect::<Vec<_>>();
        assert_not_contains(&file_refs, "reset");
    }

    #[test]
    fn extracts_actions_through_a_middleware_wrapper_create_persist() {
        let code = r#"
      import { create } from 'zustand'
      import { persist } from 'zustand/middleware'
      export const useCounter = create(
        persist(
          (set, get) => ({
            value: 0,
            increment: () => set({ value: get().value + 1 }),
          }),
          { name: 'counter' }
        )
      )
    "#;
        let result = extract_from_source("counter.ts", code, None, None);
        let fn_names = function_names(&result);
        assert_contains(&fn_names, "increment");
    }

    #[test]
    fn extracts_actions_when_the_initializer_returns_via_a_block_return_object() {
        let code = r#"
      import { create } from 'zustand'
      export const useThing = create((set) => {
        const initial = 0
        return {
          value: initial,
          bump: () => set({ value: 1 }),
        }
      })
    "#;
        let result = extract_from_source("thing.ts", code, None, None);
        let fn_names = function_names(&result);
        assert_contains(&fn_names, "bump");
    }

    #[test]
    fn does_not_extract_methods_from_a_non_exported_call_wrapped_object_noise_gate() {
        let code = r#"
      function wrap(f: any) { return f }
      const local = wrap(() => ({ shouldNotExtract: () => {} }))
    "#;
        let result = extract_from_source("inline.ts", code, None, None);
        let names = node_names(&result);
        assert_not_contains(&names, "shouldNotExtract");
    }

    #[test]
    fn still_extracts_the_existing_direct_object_shape_export_const_actions_object() {
        let code = r#"
      export const actions = {
        load: async () => { helper() },
      }
      function helper() {}
    "#;
        let result = extract_from_source("actions.ts", code, None, None);
        let fn_names = function_names(&result);
        assert_contains(&fn_names, "load");
    }
}

mod object_literal_method_resolution_end_to_end {
    use super::*;

    #[test]
    fn resolves_callers_of_store_actions_across_files_destructured_plus_chained_get_state() {
        let project = TempProject::new("cg-store");
        project.write(
            "package.json",
            "{\"name\":\"t\",\"dependencies\":{\"zustand\":\"^4\"}}\n",
        );
        project.write(
            "store.ts",
            "import { create } from 'zustand'\n\
             interface S { fetchUser(): Promise<void>; reset(): void }\n\
             export const useStore = create<S>((set, get) => ({\n\
               fetchUser: async () => { get().reset() },\n\
               reset: () => set({}),\n\
             }))\n",
        );
        project.write(
            "caller.ts",
            "import { useStore } from './store'\n\
             export async function loginFlow() {\n\
               const { fetchUser } = useStore.getState()\n\
               await fetchUser()\n\
             }\n\
             export function hardReset() {\n\
               useStore.getState().reset()\n\
             }\n",
        );

        let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);

        let fns = cg.get_nodes_by_kind(NodeKind::Function);
        let fetch_user = fns
            .iter()
            .find(|node| node.name == "fetchUser" && node.file_path.ends_with("store.ts"))
            .expect("fetchUser should be indexed");
        let reset = fns
            .iter()
            .find(|node| node.name == "reset" && node.file_path.ends_with("store.ts"))
            .expect("reset should be indexed");

        // Destructured-then-bare call: loginFlow -> fetchUser
        let fetch_user_callers = cg
            .get_callers(&fetch_user.id, 1)
            .into_iter()
            .map(|caller| caller.node.name)
            .collect::<Vec<_>>();
        assert_contains(&fetch_user_callers, "loginFlow");

        // Chained getState() call: hardReset -> reset, AND in-store sibling:
        // fetchUser -> reset
        let reset_callers = cg
            .get_callers(&reset.id, 1)
            .into_iter()
            .map(|caller| caller.node.name)
            .collect::<Vec<_>>();
        assert_contains(&reset_callers, "hardReset");
        assert_contains(&reset_callers, "fetchUser");

        cg.close();
    }
}
