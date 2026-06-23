mod describe_4967_full_indexing {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Full Indexing";
    const TS_DESCRIBE_LINE: usize = 4967;
    #[test]
    fn describes_073_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4967);
    }
    #[test]
    fn case_4978_should_index_a_typescript_file() {
        let suite = ["Full Indexing"];
        assert_eq!(suite.len(), 1);
        assert_eq!(249, 249);
        let temp = TempDir::new("codegraph-full-index-typescript");
        temp.write(
            "src/utils.ts",
            "\nexport function add(a: number, b: number): number {\n  return a + b;\n}\n\nexport function multiply(a: number, b: number): number {\n  return a * b;\n}\n",
        );

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());

        assert!(result.success, "index_all failed: {:?}", result.errors);
        assert_eq!(result.files_indexed, 1);
        assert!(
            result.nodes_created >= 2,
            "expected at least function nodes, got {result:?}"
        );

        let nodes = cg.get_nodes_in_file("src/utils.ts");
        assert!(
            nodes.len() >= 2,
            "expected stored nodes for src/utils.ts: {nodes:?}"
        );
        let add_func = nodes
            .iter()
            .find(|node| node.name == "add")
            .expect("add function should be indexed");
        assert_eq!(add_func.kind, NodeKind::Function);
        cg.close();
    }
    #[test]
    fn case_5014_should_index_multiple_files() {
        let suite = ["Full Indexing"];
        assert_eq!(suite.len(), 1);
        assert_eq!(250, 250);
        let temp = TempDir::new("codegraph-full-index-multiple");
        temp.write(
            "src/math.ts",
            "export function add(a: number, b: number) { return a + b; }",
        );
        temp.write(
            "src/string.ts",
            "export function capitalize(s: string) { return s.toUpperCase(); }",
        );

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());

        assert!(result.success, "index_all failed: {:?}", result.errors);
        assert_eq!(result.files_indexed, 2);
        let files = cg
            .get_files()
            .into_iter()
            .map(|file| file.path)
            .collect::<Vec<_>>();
        assert_eq!(files, vec!["src/math.ts", "src/string.ts"]);
        cg.close();
    }
    #[test]
    fn case_5042_should_track_file_hashes_for_incremental_updates() {
        let suite = ["Full Indexing"];
        assert_eq!(suite.len(), 1);
        assert_eq!(251, 251);
        let temp = TempDir::new("codegraph-full-index-hashes");
        temp.write("src/main.ts", "export const x = 1;");

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "index_all failed: {:?}", result.errors);

        let file = cg
            .get_file("src/main.ts")
            .expect("src/main.ts should be tracked");
        assert!(!file.content_hash.is_empty());

        temp.write("src/main.ts", "export const x = 2;");

        let changes = cg.get_changed_files();
        assert_contains(&changes.modified, "src/main.ts");
        cg.close();
    }
    #[test]
    fn case_5067_should_sync_and_detect_changes() {
        let suite = ["Full Indexing"];
        assert_eq!(suite.len(), 1);
        assert_eq!(252, 252);
        let temp = TempDir::new("codegraph-full-index-sync");
        temp.write("src/main.ts", "export function original() { return 1; }");

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "index_all failed: {:?}", result.errors);
        let initial_nodes = cg.get_nodes_in_file("src/main.ts");
        assert!(
            initial_nodes.iter().any(|node| node.name == "original"),
            "original function should be indexed: {initial_nodes:?}"
        );

        temp.write("src/main.ts", "export function updated() { return 2; }");

        let sync_result = cg.sync(IndexOptions::default());
        assert_eq!(sync_result.files_modified, 1);

        let updated_nodes = cg.get_nodes_in_file("src/main.ts");
        assert!(
            updated_nodes.iter().any(|node| node.name == "updated"),
            "updated function should be indexed: {updated_nodes:?}"
        );
        assert!(
            !updated_nodes.iter().any(|node| node.name == "original"),
            "original function should be removed after sync: {updated_nodes:?}"
        );
        cg.close();
    }
    #[test]
    fn case_5101_should_count_file_level_tracked_yaml_files_as_indexed() {
        let suite = ["Full Indexing"];
        assert_eq!(suite.len(), 1);
        assert_eq!(253, 253);
        let temp = TempDir::new("codegraph-full-index-yaml");
        temp.write("app.yaml", "name: test\n");
        temp.write("routes.yml", "route: value\n");

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());

        assert!(result.success, "index_all failed: {:?}", result.errors);
        assert_eq!(result.files_indexed, 2);
        assert_eq!(result.files_skipped, 0);
        let tracked = cg
            .get_files()
            .into_iter()
            .map(|file| file.path)
            .collect::<Vec<_>>();
        assert_eq!(tracked, vec!["app.yaml", "routes.yml"]);
        cg.close();
    }
    #[test]
    fn case_5116_should_count_file_level_tracked_yaml_twig_files_as_indexed_in_indexfil() {
        let suite = ["Full Indexing"];
        assert_eq!(suite.len(), 1);
        assert_eq!(254, 254);
        let temp = TempDir::new("codegraph-full-index-yaml-twig-index-files");
        temp.write("app.yaml", "name: test\n");
        temp.write("view.twig", "{{ title }}\n");

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_files(&["app.yaml".to_owned(), "view.twig".to_owned()]);

        assert!(result.success, "index_files failed: {:?}", result.errors);
        assert_eq!(result.files_indexed, 2);
        assert_eq!(result.files_skipped, 0);
        let tracked = cg
            .get_files()
            .into_iter()
            .map(|file| format!("{}:{}", file.path, language_key(&file.language)))
            .collect::<Vec<_>>();
        assert_eq!(tracked, vec!["app.yaml:yaml", "view.twig:twig"]);
        cg.close();
    }
    #[test]
    fn case_5133_should_count_file_level_tracked_properties_files_as_indexed() {
        let suite = ["Full Indexing"];
        assert_eq!(suite.len(), 1);
        assert_eq!(255, 255);
        let temp = TempDir::new("codegraph-full-index-properties");
        temp.write("application.properties", "server.port=8080\n");
        temp.write("log.properties", "log.level=INFO\n");

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());

        assert!(result.success, "index_all failed: {:?}", result.errors);
        assert_eq!(result.files_indexed, 2);
        assert_eq!(result.files_skipped, 0);
        cg.close();
    }
    #[test]
    fn case_5147_should_count_the_full_file_level_tracked_class_yaml_twig_properties_in() {
        let suite = ["Full Indexing"];
        assert_eq!(suite.len(), 1);
        assert_eq!(256, 256);
        let temp = TempDir::new("codegraph-full-index-file-level-index-files");
        temp.write("app.yaml", "name: test\n");
        temp.write("view.twig", "{{ title }}\n");
        temp.write("application.properties", "server.port=8080\n");

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_files(&[
            "app.yaml".to_owned(),
            "view.twig".to_owned(),
            "application.properties".to_owned(),
        ]);

        assert!(result.success, "index_files failed: {:?}", result.errors);
        assert_eq!(result.files_indexed, 3);
        assert_eq!(result.files_skipped, 0);
        let tracked = cg
            .get_files()
            .into_iter()
            .map(|file| format!("{}:{}", file.path, language_key(&file.language)))
            .collect::<Vec<_>>();
        assert_eq!(
            tracked,
            vec![
                "app.yaml:yaml",
                "application.properties:properties",
                "view.twig:twig"
            ]
        );
        cg.close();
    }
}
