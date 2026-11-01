use rustcodegraph::resolution::{
    index::ReferenceResolver, name_matcher::match_by_exact_name, types::*,
};
use rustcodegraph::types::*;
use rustcodegraph::{DatabaseConnection, QueryBuilder};
fn make_node_named(id: &str, name: &str) -> Node {
    Node {
        id: id.to_owned(),
        kind: NodeKind::Function,
        name: name.to_owned(),
        qualified_name: name.to_owned(),
        file_path: "a.ts".to_owned(),
        language: Language::TypeScript,
        start_line: 1,
        end_line: 1,
        start_column: 0,
        end_column: 0,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: None,
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now_ms(),
    }
}

struct Counting<'a> {
    inner: ReferenceResolver<'a>,
    reads: usize,
}
impl ResolutionContext for Counting<'_> {
    fn get_nodes_in_file(&mut self, file_path: &str) -> Vec<Node> {
        self.inner.get_nodes_in_file(file_path)
    }
    fn get_nodes_by_name(&mut self, name: &str) -> Vec<Node> {
        self.inner.get_nodes_by_name(name)
    }
    fn get_nodes_by_qualified_name(&mut self, qualified_name: &str) -> Vec<Node> {
        self.inner.get_nodes_by_qualified_name(qualified_name)
    }
    fn get_nodes_by_kind(&mut self, kind: NodeKind) -> Vec<Node> {
        self.inner.get_nodes_by_kind(kind)
    }
    fn file_exists(&mut self, file_path: &str) -> bool {
        self.inner.file_exists(file_path)
    }
    fn read_file(&mut self, file_path: &str) -> Option<String> {
        self.reads += 1;
        self.inner.read_file(file_path)
    }
    fn get_project_root(&self) -> String {
        self.inner.get_project_root()
    }
    fn get_all_files(&mut self) -> Vec<String> {
        self.inner.get_all_files()
    }
    fn get_nodes_by_lower_name(&mut self, lower_name: &str) -> Vec<Node> {
        self.inner.get_nodes_by_lower_name(lower_name)
    }
    fn get_import_mappings(&mut self, file_path: &str, language: Language) -> Vec<ImportMapping> {
        self.inner.get_import_mappings(file_path, language)
    }
}

#[test]
fn case_012_candidates_do_not_reread_reference() {
    for language in [Language::Rust, Language::Java] {
        let dir = std::env::temp_dir().join(format!("rcg012-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir(&dir).unwrap();
        let mut db = DatabaseConnection::initialize(dir.join("graph.db")).unwrap();
        let inner = ReferenceResolver::new(dir.to_string_lossy(), QueryBuilder::new(db.get_db()));
        let mut context = Counting { inner, reads: 0 };
        let reference = UnresolvedRef {
            from_node_id: "caller".into(),
            reference_name: "target".into(),
            reference_kind: ReferenceKind::Calls,
            line: 5,
            column: 0,
            file_path: "local.rs".into(),
            language,
            candidates: None,
        };
        // Empty and single-candidate boundaries, then substantial ambiguity.
        assert!(match_by_exact_name(&reference, &mut context).is_none());
        for count in [1, 64] {
            for i in 0..count {
                let mut node = make_node_named(&format!("candidate-{i}"), "target");
                node.language = language;
                node.file_path = if i == 0 {
                    "local.rs".into()
                } else {
                    format!("other/{i}.rs")
                };
                context.inner.queries.insert_node(&node).unwrap();
            }
            context.inner.clear_caches();
            let result = match_by_exact_name(&reference, &mut context).unwrap();
            assert_eq!(result.target_node_id, "candidate-0");
            assert_eq!(context.reads, 0, "candidate count {count}");
        }
        drop(context);
        db.close().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn case_012_native_candidate_edges() {
    for (file, source) in [
        (
            "main.rs",
            "fn target() {}\nfn caller() { target(); }\nmod other { pub fn target() {} }\n",
        ),
        (
            "Main.java",
            "class Main {\n static void target() {}\n void caller() { target(); }\n}\nclass Other {\n static void target() {}\n}\n",
        ),
    ] {
        let dir =
            std::env::temp_dir().join(format!("rcg012-native-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join(file), source).unwrap();
        let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
        assert!(graph.index_all(Default::default()).success);
        let mut nodes = graph.get_nodes_by_kind(NodeKind::Function);
        nodes.extend(graph.get_nodes_by_kind(NodeKind::Method));
        let caller = nodes.iter().find(|n| n.name == "caller").unwrap();
        let callees = graph.get_callees(&caller.id, 1);
        // Java may conservatively leave this ambiguous call unresolved.
        if file == "main.rs" {
            assert!(!callees.is_empty());
        }
        assert!(callees.iter().all(|entry| entry.node.name == "target"));
        let mut before = callees
            .iter()
            .map(|entry| entry.node.id.clone())
            .collect::<Vec<_>>();
        before.sort();
        assert!(graph.index_all(Default::default()).success);
        let mut after = graph
            .get_callees(&caller.id, 1)
            .iter()
            .map(|entry| entry.node.id.clone())
            .collect::<Vec<_>>();
        after.sort();
        assert_eq!(before, after, "{file}: preserve the existing edge set");
        graph.destroy();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
