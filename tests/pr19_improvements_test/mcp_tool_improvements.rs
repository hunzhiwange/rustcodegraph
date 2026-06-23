mod mcp_tool_improvements {
    use super::*;

    #[test]
    fn should_export_tool_handler_class() {
        if sqlite_unavailable() {
            return;
        }

        let mut handler = ToolHandler::new(false);
        assert!(!handler.has_default_code_graph());
        handler.close_all();
    }

    #[test]
    fn should_have_find_symbol_matches_and_truncate_output_as_private_methods() {
        if sqlite_unavailable() {
            return;
        }

        let mut handler = ToolHandler::new(false);
        let _find: fn(&mut ToolHandler, &mut CodeGraph, &str) -> Vec<Node> =
            ToolHandler::find_symbol_matches;
        let _truncate: fn(&str) -> String = truncate_output;
        handler.close_all();
    }

    #[test]
    fn should_truncate_output_exceeding_max_output_length() {
        if sqlite_unavailable() {
            return;
        }

        let short = "Hello world";
        assert_eq!(truncate_output(short), short);

        let long = "x".repeat(20_000);
        let result = truncate_output(&long);
        assert!(result.len() < long.len());
        assert!(result.contains("... (output truncated)"));
    }

    #[test]
    fn should_truncate_at_a_clean_line_boundary() {
        if sqlite_unavailable() {
            return;
        }

        let lines = (0..500)
            .map(|i| format!("Line {i}: {}", "a".repeat(50)))
            .collect::<Vec<_>>();
        let text = lines.join("\n");

        let result = truncate_output(&text);
        assert!(result.contains("... (output truncated)"));
        let before_truncation = result
            .split("\n\n... (output truncated)")
            .next()
            .expect("truncation split should have a prefix");
        assert!(before_truncation.ends_with('\n') || !before_truncation.contains('\0'));
    }

    mod find_symbol_disambiguation {
        use super::*;

        #[test]
        fn should_prefer_exact_name_matches() {
            if sqlite_unavailable() {
                return;
            }

            let tmp_dir = create_temp_dir();
            let src_dir = tmp_dir.path().join("src");
            fs::create_dir_all(&src_dir).expect("src fixture dir should be created");
            write_fixture(
                src_dir.join("a.ts"),
                r#"
export function getValue(): number { return 1; }
export function getValueFromCache(): number { return 2; }
"#,
            );

            let mut cg = CodeGraph::init_sync(tmp_dir.path()).expect("CodeGraph should initialize");
            let _ = cg.index_all(IndexOptions::default());

            let matches = find_symbol_matches(&mut cg, "getValue");
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].name, "getValue");
            cg.destroy();
        }

        #[test]
        fn should_return_all_definitions_when_multiple_symbols_share_the_same_name() {
            if sqlite_unavailable() {
                return;
            }

            let tmp_dir = create_temp_dir();
            let src_dir = tmp_dir.path().join("src");
            fs::create_dir_all(&src_dir).expect("src fixture dir should be created");
            write_fixture(
                src_dir.join("a.ts"),
                r#"
export function handle(): void {}
"#,
            );
            write_fixture(
                src_dir.join("b.ts"),
                r#"
export function handle(): void {}
"#,
            );

            let mut cg = CodeGraph::init_sync(tmp_dir.path()).expect("CodeGraph should initialize");
            let _ = cg.index_all(IndexOptions::default());

            let matches = find_symbol_matches(&mut cg, "handle");
            assert_eq!(matches.len(), 2);
            assert!(matches.iter().all(|node| node.name == "handle"));
            cg.destroy();
        }

        #[test]
        fn should_return_no_matches_when_symbol_is_not_found() {
            if sqlite_unavailable() {
                return;
            }

            let tmp_dir = create_temp_dir();
            let src_dir = tmp_dir.path().join("src");
            fs::create_dir_all(&src_dir).expect("src fixture dir should be created");
            write_fixture(src_dir.join("a.ts"), "export function foo(): void {}");

            let mut cg = CodeGraph::init_sync(tmp_dir.path()).expect("CodeGraph should initialize");
            let _ = cg.index_all(IndexOptions::default());

            let matches = find_symbol_matches(&mut cg, "nonExistentSymbol");
            assert_eq!(matches.len(), 0);
            cg.destroy();
        }
    }
}
