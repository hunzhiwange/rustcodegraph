mod cli_uninit {
    use super::*;

    #[test]
    fn should_uninitialize_a_project_via_code_graph_uninitialize() {
        if sqlite_unavailable() {
            return;
        }

        let test_dir = create_temp_dir();
        let mut cg = CodeGraph::init_sync(test_dir.path()).expect("CodeGraph should initialize");
        assert!(CodeGraph::is_initialized(test_dir.path()));

        cg.uninitialize().expect("project should uninitialize");
        assert!(!CodeGraph::is_initialized(test_dir.path()));
    }
}
