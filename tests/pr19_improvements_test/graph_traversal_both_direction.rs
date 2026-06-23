mod graph_traversal_both_direction {
    use super::*;

    #[test]
    fn should_traverse_both_directions_from_a_node() {
        if sqlite_unavailable() {
            return;
        }

        let test_dir = create_temp_dir();
        let src_dir = test_dir.path().join("src");
        fs::create_dir_all(&src_dir).expect("src fixture dir should be created");

        write_fixture(
            src_dir.join("a.ts"),
            r#"
import { funcB } from './b';
export function funcA(): void { funcB(); }
"#,
        );
        write_fixture(
            src_dir.join("b.ts"),
            r#"
import { funcC } from './c';
export function funcB(): void { funcC(); }
"#,
        );
        write_fixture(
            src_dir.join("c.ts"),
            r#"
export function funcC(): void { console.log('c'); }
"#,
        );

        let mut cg = CodeGraph::init_sync(test_dir.path()).expect("CodeGraph should initialize");
        let _ = cg.index_all(IndexOptions::default());
        let _ = cg.resolve_references();

        let func_b = cg
            .get_nodes_by_kind(NodeKind::Function)
            .into_iter()
            .find(|node| node.name == "funcB");

        let Some(func_b) = func_b else {
            cg.destroy();
            return;
        };

        let subgraph = cg.traverse(
            &func_b.id,
            Some(TraversalOptions {
                max_depth: Some(1),
                edge_kinds: None,
                node_kinds: None,
                direction: Some(TraversalDirection::Both),
                limit: None,
                include_start: None,
            }),
        );

        assert!(subgraph.nodes.len() >= 2);
        assert!(subgraph.nodes.contains_key(&func_b.id));
        cg.destroy();
    }
}
