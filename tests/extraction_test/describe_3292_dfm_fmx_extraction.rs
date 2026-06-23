mod describe_3292_dfm_fmx_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "DFM/FMX Extraction";
    const TS_DESCRIBE_LINE: usize = 3292;
    #[test]
    fn describes_050_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 3292);
    }
    #[test]
    fn case_3293_should_extract_components_from_dfm() {
        let suite = ["DFM/FMX Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(208, 208);
        let code = r#"object Form1: TForm1
  Left = 0
  Top = 0
  Caption = 'My Form'
  object Button1: TButton
    Left = 10
    Top = 10
    Caption = 'Click Me'
  end
end"#;
        let result = extract("Form1.dfm", code);
        let components = names_by_kind(&result, NodeKind::Component);
        assert_eq!(components.len(), 2, "components: {components:?}");
        assert_contains(&components, "Form1");
        assert_contains(&components, "Button1");
        assert_signature_eq(
            find_node(&result, NodeKind::Component, "Button1").expect("Button1 should exist"),
            "TButton",
        );
    }
    #[test]
    fn case_3315_should_extract_nested_component_hierarchy() {
        let suite = ["DFM/FMX Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(209, 209);
        let code = r#"object Form1: TForm1
  object Panel1: TPanel
    object Label1: TLabel
      Caption = 'Hello'
    end
  end
end"#;
        let result = extract("Form1.dfm", code);
        assert_eq!(nodes_by_kind(&result, NodeKind::Component).len(), 3);
        let panel = find_node(&result, NodeKind::Component, "Panel1").expect("Panel1 should exist");
        let label = find_node(&result, NodeKind::Component, "Label1").expect("Label1 should exist");
        assert!(
            result.edges.iter().any(|edge| {
                edge.source == panel.id
                    && edge.target == label.id
                    && edge.kind == EdgeKind::Contains
            }),
            "edges: {:?}",
            result.edges
        );
    }
    #[test]
    fn case_3337_should_extract_event_handler_references() {
        let suite = ["DFM/FMX Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(210, 210);
        let code = r#"object Form1: TForm1
  OnCreate = FormCreate
  OnDestroy = FormDestroy
  object Button1: TButton
    OnClick = Button1Click
  end
end"#;
        let result = extract("Form1.dfm", code);
        assert_eq!(result.unresolved_references.len(), 3);
        let refs = references_by_kind(&result, ReferenceKind::References);
        assert_contains(&refs, "FormCreate");
        assert_contains(&refs, "FormDestroy");
        assert_contains(&refs, "Button1Click");
    }
    #[test]
    fn case_3355_should_handle_multi_line_properties() {
        let suite = ["DFM/FMX Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(211, 211);
        let code = r#"object Form1: TForm1
  SQL.Strings = (
    'SELECT * FROM users'
    'WHERE active = 1')
  object Button1: TButton
    OnClick = Button1Click
  end
end"#;
        let result = extract("Form1.dfm", code);
        assert_eq!(nodes_by_kind(&result, NodeKind::Component).len(), 2);
        let refs = references_by_kind(&result, ReferenceKind::References);
        assert_eq!(refs, ["Button1Click"]);
    }
    #[test]
    fn case_3374_should_handle_inherited_keyword() {
        let suite = ["DFM/FMX Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(212, 212);
        let code = r#"inherited Form1: TForm1
  Caption = 'Inherited Form'
  object Button1: TButton
    OnClick = Button1Click
  end
end"#;
        let result = extract("Form1.dfm", code);
        let components = names_by_kind(&result, NodeKind::Component);
        assert_eq!(components.len(), 2, "components: {components:?}");
        assert_contains(&components, "Form1");
    }
    #[test]
    fn case_3388_should_handle_item_collection_properties() {
        let suite = ["DFM/FMX Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(213, 213);
        let code = r#"object Form1: TForm1
  object StatusBar1: TStatusBar
    Panels = <
      item
        Width = 200
      end
      item
        Width = 200
      end>
  end
end"#;
        let result = extract("Form1.dfm", code);
        assert_eq!(nodes_by_kind(&result, NodeKind::Component).len(), 2);
    }
    mod describe_3406_full_fixture_mainform_dfm {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Full fixture: MainForm.dfm";
        const TS_DESCRIBE_LINE: usize = 3406;
        const CODE: &str = r#"object frmMain: TfrmMain
  Left = 0
  Top = 0
  Caption = 'CodeGraph DFM Fixture'
  ClientHeight = 480
  ClientWidth = 640
  OnCreate = FormCreate
  OnDestroy = FormDestroy
  object pnlTop: TPanel
    Left = 0
    Top = 0
    Width = 640
    Height = 50
    object lblTitle: TLabel
      Left = 16
      Top = 16
      Caption = 'Authentication Service'
    end
    object btnLogin: TButton
      Left = 540
      Top = 12
      OnClick = btnLoginClick
    end
  end
  object pnlContent: TPanel
    Left = 0
    Top = 50
    object edtUsername: TEdit
      Left = 16
      Top = 16
      OnChange = edtUsernameChange
    end
    object edtPassword: TEdit
      Left = 16
      Top = 48
      OnKeyPress = edtPasswordKeyPress
    end
    object mmoLog: TMemo
      Left = 16
      Top = 88
    end
  end
  object pnlStatus: TStatusBar
    Left = 0
    Top = 440
    Panels = <
      item
        Width = 200
      end
      item
        Width = 200
      end>
  end
end"#;

        #[test]
        fn describes_051_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 3406);
        }
        #[test]
        fn case_3462_should_extract_all_components() {
            let suite = ["DFM/FMX Extraction", "Full fixture: MainForm.dfm"];
            assert_eq!(suite.len(), 2);
            assert_eq!(214, 214);
            let result = extract("MainForm.dfm", CODE);
            let components = names_by_kind(&result, NodeKind::Component);
            assert_eq!(components.len(), 9, "components: {components:?}");
            for name in [
                "frmMain",
                "pnlTop",
                "lblTitle",
                "btnLogin",
                "pnlContent",
                "edtUsername",
                "edtPassword",
                "mmoLog",
                "pnlStatus",
            ] {
                assert_contains(&components, name);
            }
        }
        #[test]
        fn case_3475_should_extract_all_event_handlers() {
            let suite = ["DFM/FMX Extraction", "Full fixture: MainForm.dfm"];
            assert_eq!(suite.len(), 2);
            assert_eq!(215, 215);
            let result = extract("MainForm.dfm", CODE);
            let refs = references_by_kind(&result, ReferenceKind::References);
            assert_eq!(refs.len(), 5, "refs: {refs:?}");
            for name in [
                "FormCreate",
                "FormDestroy",
                "btnLoginClick",
                "edtUsernameChange",
                "edtPasswordKeyPress",
            ] {
                assert_contains(&refs, name);
            }
        }
    }
}
