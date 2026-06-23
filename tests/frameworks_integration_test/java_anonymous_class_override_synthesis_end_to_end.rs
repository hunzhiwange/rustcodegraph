use super::*;

#[test]
fn bridges_an_abstract_base_method_to_overrides_inside_new_base() {
    let project = TempProject::new("cg-anon-java");
    project.write(
        "Splitter.java",
        "package com.example;\n\
         \n\
         abstract class BaseIter {\n\
           abstract int separatorStart(int start);\n\
         }\n\
         \n\
         public class Splitter {\n\
           public BaseIter make() {\n\
             return new BaseIter() {\n\
               @Override\n\
               int separatorStart(int start) { return start + 1; }\n\
             };\n\
           }\n\
         }\n",
    );

    let mut cg = index(&project);

    let anon_class = cg
        .get_nodes_by_kind(NodeKind::Class)
        .into_iter()
        .find(|node| node.name.contains("BaseIter$anon@"));
    assert!(
        anon_class.is_some(),
        "anonymous BaseIter subclass should be a class node"
    );

    let base_abstract = cg
        .get_nodes_by_kind(NodeKind::Method)
        .into_iter()
        .find(|node| node.qualified_name == "com.example::BaseIter::separatorStart")
        .expect("base abstract method should be in the graph");
    let anon_override = cg
        .get_nodes_by_kind(NodeKind::Method)
        .into_iter()
        .find(|node| {
            node.name == "separatorStart"
                && node.qualified_name.contains("$anon@")
                && node
                    .qualified_name
                    .starts_with("com.example::Splitter::make::")
        })
        .expect("anon-class override should be in the graph");

    let synth_edge = cg
        .get_outgoing_edges(&base_abstract.id)
        .into_iter()
        .find(|edge| edge.target == anon_override.id && edge.kind == EdgeKind::Calls)
        .expect("BaseIter.separatorStart should bridge to anon.separatorStart");
    assert_eq!(synth_edge.provenance, Some(EdgeProvenance::Heuristic));
    assert_eq!(
        edge_metadata_str(&synth_edge, "synthesizedBy"),
        Some("interface-impl")
    );

    cg.close();
}
