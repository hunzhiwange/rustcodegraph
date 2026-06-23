mod describe_6974_java_annotations_blast_radius_recall {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Java annotations (blast-radius recall)";
    const TS_DESCRIBE_LINE: usize = 6974;
    #[test]
    fn describes_110_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 6974);
    }
    #[test]
    fn case_6975_indexes_interface_definitions_and_links_annotation_usages_to_them() {
        let suite = ["Java annotations (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(363, 363);
        let temp = TempDir::new("codegraph-java-annotation-deps");
        temp.write(
            "p/MyAnno.java",
            "package p;\npublic @interface MyAnno { String value() default \"\"; }\n",
        );
        temp.write(
            "p/User.java",
            "package p;\n@MyAnno(\"c\")\npublic class User {\n  @MyAnno(\"f\") int field;\n  @MyAnno(\"m\") void go() {}\n}\n",
        );

        let mut cg = index_project(&temp);
        let anno = cg
            .get_nodes_by_kind(NodeKind::Interface)
            .into_iter()
            .find(|node| node.name == "MyAnno")
            .expect("@interface MyAnno should be indexed");
        let impacted = impact_file_paths(&mut cg, &anno.id, 2);
        assert!(
            impacted.iter().any(|path| path.ends_with("p/User.java")),
            "@MyAnno usages should depend on the annotation definition, impacted: {impacted:?}"
        );
        cg.close();
    }
}
