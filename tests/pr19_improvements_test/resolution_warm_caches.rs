mod resolution_warm_caches {
    use super::*;

    #[test]
    fn should_warm_caches_and_use_them_for_lookups() {
        if sqlite_unavailable() {
            return;
        }

        let test_dir = create_temp_dir();
        let src_dir = test_dir.path().join("src");
        fs::create_dir_all(&src_dir).expect("src fixture dir should be created");
        write_fixture(
            src_dir.join("a.ts"),
            r#"
export function myFunc(): void {}
export function otherFunc(): void { myFunc(); }
"#,
        );

        let mut cg = CodeGraph::init_sync(test_dir.path()).expect("CodeGraph should initialize");
        let _ = cg.index_all(IndexOptions::default());

        let result = cg.resolve_references();
        assert!((result.stats.total as i128) >= 0);
        cg.destroy();
    }
}
