mod describe_6995_swift_property_wrappers_attributes_blast_radius_recall {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Swift property wrappers / attributes (blast-radius recall)";
    const TS_DESCRIBE_LINE: usize = 6995;
    #[test]
    fn describes_111_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 6995);
    }
    #[test]
    fn case_6996_links_a_propertywrapper_usage_to_the_wrapper_type() {
        let suite = ["Swift property wrappers / attributes (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(364, 364);
        let temp = TempDir::new("codegraph-swift-property-wrapper-deps");
        temp.write(
            "Sources/M/Wrap.swift",
            "@propertyWrapper\npublic struct Argument<T> { public var wrappedValue: T }\n",
        );
        temp.write(
            "Sources/M/Cmd.swift",
            "public struct MyCommand {\n  @Argument var name: String\n  @Argument var count: Int\n}\n",
        );

        let mut cg = index_project(&temp);
        let wrapper = cg
            .get_nodes_by_kind(NodeKind::Struct)
            .into_iter()
            .find(|node| node.name == "Argument")
            .expect("property wrapper struct should be indexed");
        let impacted = impact_file_paths(&mut cg, &wrapper.id, 2);
        assert!(
            impacted
                .iter()
                .any(|path| path.ends_with("Sources/M/Cmd.swift")),
            "@Argument usage should depend on wrapper type, impacted: {impacted:?}"
        );
        cg.close();
    }
}
