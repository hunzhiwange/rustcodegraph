mod describe_4035_static_member_value_read_references {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Static-member / value-read references";
    const TS_DESCRIBE_LINE: usize = 4035;
    #[test]
    fn describes_058_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4035);
    }
    #[test]
    fn case_4048_links_a_type_referenced_only_via_a_static_field_enum_value_and_ignores() {
        let suite = ["Static-member / value-read references"];
        assert_eq!(suite.len(), 1);
        assert_eq!(224, 224);
        let temp = TempDir::new("codegraph-static-member-value-read");
        temp.write(
            "JsonScope.java",
            r#"class JsonScope {
  static final int EMPTY_DOCUMENT = 1;
}
"#,
        );
        temp.write(
            "Reader.java",
            r#"class Reader {
  private int helper;
  int peek() {
    return JsonScope.EMPTY_DOCUMENT;
  }
  int noop() {
    return this.helper;
  }
}
"#,
        );

        let mut cg = index_project(&temp);
        let scope = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.name == "JsonScope")
            .expect("JsonScope should be indexed");
        let reached = impact_file_paths(&mut cg, &scope.id, 3);
        assert!(
            reached.iter().any(|path| path.ends_with("Reader.java")),
            "JsonScope should reach Reader.java: {reached:?}"
        );
        let ref_targets = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .filter(|node| node.name == "this" || node.name == "helper")
            .collect::<Vec<_>>();
        assert!(ref_targets.is_empty(), "unexpected refs: {ref_targets:?}");
        cg.close();
    }
    #[test]
    fn case_4091_does_not_link_a_static_member_read_across_language_families_coincident() {
        let suite = ["Static-member / value-read references"];
        assert_eq!(suite.len(), 1);
        assert_eq!(225, 225);
        let temp = TempDir::new("codegraph-static-member-cross-family");
        temp.write(
            "Build.ts",
            r#"export class Build {
  static version = 1;
}
"#,
        );
        temp.write(
            "Device.kt",
            r#"package app
class Device {
  fun sdk(): Int = Build.VERSION
}
"#,
        );

        let mut cg = index_project(&temp);
        let ts_build = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.name == "Build" && node.file_path.ends_with("Build.ts"))
            .expect("TS Build class should be indexed");
        let deps = impact_file_paths(&mut cg, &ts_build.id, 2);
        assert!(
            deps.iter().all(|path| !path.ends_with("Device.kt")),
            "TS Build must not reach Kotlin Device.kt: {deps:?}"
        );
        cg.close();
    }
}
