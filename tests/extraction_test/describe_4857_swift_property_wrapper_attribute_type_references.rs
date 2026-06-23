mod describe_4857_swift_property_wrapper_attribute_type_references {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Swift property-wrapper attribute type references";
    const TS_DESCRIBE_LINE: usize = 4857;
    #[test]
    fn describes_071_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4857);
    }
    #[test]
    fn case_4870_a_fluent_siblings_through_pivot_self_links_the_model_to_the_pivot_type() {
        let suite = ["Swift property-wrapper attribute type references"];
        assert_eq!(suite.len(), 1);
        assert_eq!(247, 247);
        let temp = TempDir::new("codegraph-swift-fluent-siblings");
        temp.write(
            "Pivot.swift",
            "import Fluent\nfinal class AcronymCategoryPivot: Model {\n  static let schema = \"acronym-category\"\n}\n",
        );
        temp.write(
            "Acronym.swift",
            "import Fluent\nfinal class Acronym: Model {\n  @Siblings(through: AcronymCategoryPivot.self, from: \\.$acronym, to: \\.$category)\n  var categories: [Category]\n}\n",
        );

        let mut cg = index_project(&temp);
        let pivot = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.name == "AcronymCategoryPivot")
            .expect("pivot model class should be indexed");
        let impacted = impact_file_paths(&mut cg, &pivot.id, 2);
        assert!(
            impacted.iter().any(|path| path.ends_with("Acronym.swift")),
            "@Siblings metatype arg should link Acronym to the pivot, impacted: {impacted:?}"
        );
        cg.close();
    }
}
