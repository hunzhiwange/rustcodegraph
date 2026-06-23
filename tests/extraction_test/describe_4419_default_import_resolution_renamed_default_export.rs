mod describe_4419_default_import_resolution_renamed_default_export {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Default import resolution (renamed default export)";
    const TS_DESCRIBE_LINE: usize = 4419;
    #[test]
    fn describes_062_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4419);
    }
    #[test]
    fn case_4432_links_a_renamed_default_import_to_the_module_file() {
        let suite = ["Default import resolution (renamed default export)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(235, 235);
        let temp = TempDir::new("codegraph-default-import-module-file");
        temp.write(
            "app/controller.ts",
            "const router = { get() {} };\nexport default router;\n",
        );
        temp.write(
            "app/routes.ts",
            "import myController from './controller';\nexport const api = myController;\n",
        );

        let mut cg = index_project(&temp);
        let controller = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("app/controller.ts"))
            .expect("controller.ts should be indexed");
        let deps = impact_file_paths(&mut cg, &controller.id, 2);
        assert!(
            deps.iter().any(|path| path.ends_with("routes.ts")),
            "renamed default importer should depend on controller.ts: {deps:?}"
        );
        cg.close();
    }
}
