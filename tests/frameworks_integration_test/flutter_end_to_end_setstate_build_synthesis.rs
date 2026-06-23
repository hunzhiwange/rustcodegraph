use super::*;

#[test]
fn synthesizes_a_handler_build_edge_when_a_state_method_calls_setstate() {
    let project = TempProject::new("cg-flutter");
    project.write(
        "main.dart",
        "import \"package:flutter/material.dart\";\n\
         class CounterPage extends StatefulWidget {\n\
           @override\n\
           State<CounterPage> createState() => _CounterPageState();\n\
         }\n\
         class _CounterPageState extends State<CounterPage> {\n\
           int _count = 0;\n\
           void _increment() {\n\
             setState(() {\n\
               _count++;\n\
             });\n\
           }\n\
           @override\n\
           Widget build(BuildContext context) {\n\
             return Text(\"$_count\");\n\
           }\n\
         }\n",
    );

    let mut cg = index(&project);

    let methods = cg.get_nodes_by_kind(NodeKind::Method);
    let increment = methods
        .iter()
        .find(|node| node.name == "_increment")
        .expect("_increment should be defined");
    let build = methods
        .iter()
        .find(|node| node.name == "build")
        .expect("build should be defined");

    let edges = cg.get_outgoing_edges(&increment.id);
    let to_build = edges
        .iter()
        .find(|edge| edge.target == build.id && edge.kind == EdgeKind::Calls);
    assert!(
        to_build.is_some(),
        "_increment should reach build via setState synthesis"
    );

    cg.close();
}
