mod describe_6861_c_records_blast_radius_recall {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "C# records (blast-radius recall)";
    const TS_DESCRIBE_LINE: usize = 6861;
    #[test]
    fn describes_108_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 6861);
    }
    #[test]
    fn case_6866_extracts_a_record_as_a_graph_node_record_class_record_struct() {
        let suite = ["C# records (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(357, 357);
        let result = extract(
            "r.cs",
            "namespace P;\npublic record Box(int N);\npublic record struct Pt(int X);\n",
        );
        assert!(
            result.nodes.iter().any(|node| {
                node.name == "Box" && matches!(node.kind, NodeKind::Class | NodeKind::Struct)
            }),
            "missing Box record node: {:?}",
            result.nodes
        );
        assert!(
            result.nodes.iter().any(|node| {
                node.name == "Pt" && matches!(node.kind, NodeKind::Class | NodeKind::Struct)
            }),
            "missing Pt record node: {:?}",
            result.nodes
        );
    }
    #[test]
    fn case_6872_resolves_references_instantiations_of_a_record_across_files() {
        let suite = ["C# records (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(358, 358);
        let temp = TempDir::new("codegraph-csharp-record-dependent");
        temp.write("types.cs", "namespace P;\npublic record Box(int N);\n");
        temp.write(
            "use.cs",
            "using System.Collections.Generic;\nnamespace P;\npublic class User {\n    public IEnumerable<Box> Boxes { get; }\n    public Box Make() => new Box(1);\n}\n",
        );
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("types.cs");
        assert_contains(&dependents, "use.cs");
    }
}
