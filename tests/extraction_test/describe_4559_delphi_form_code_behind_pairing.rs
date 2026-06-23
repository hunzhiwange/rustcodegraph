mod describe_4559_delphi_form_code_behind_pairing {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Delphi form code-behind pairing";
    const TS_DESCRIBE_LINE: usize = 4559;
    #[test]
    fn describes_065_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4559);
    }
    #[test]
    fn case_4572_links_a_dfm_form_to_its_sibling_pas_code_behind_unit() {
        let suite = ["Delphi form code-behind pairing"];
        assert_eq!(suite.len(), 1);
        assert_eq!(239, 239);
        let temp = TempDir::new("codegraph-delphi-form-pairing");
        temp.write(
            "UFRMAbout.dfm",
            "object FRMAbout: TFRMAbout\n  Caption = 'About'\nend\n",
        );
        temp.write(
            "UFRMAbout.pas",
            "unit UFRMAbout;\ninterface\nuses Forms;\ntype\n  TFRMAbout = class(TForm)\n  end;\nimplementation\n{$R *.dfm}\nend.\n",
        );

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "index_all failed: {:?}", result.errors);

        let dfm = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("UFRMAbout.dfm"))
            .expect("UFRMAbout.dfm file node should exist");
        let deps = cg.get_file_dependents(&dfm.file_path);
        assert!(
            deps.iter().any(|path| path.ends_with("UFRMAbout.pas")),
            "dependents: {deps:?}"
        );
        cg.close();
    }
}
