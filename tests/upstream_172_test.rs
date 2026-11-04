use rustcodegraph::extraction::tree_sitter::TreeSitterExtractor;
use rustcodegraph::types::{EdgeKind, NodeKind};

#[test]
fn case_172_tag_declarations() {
    let source = r#"<cfcomponent>
<cffunction name="greet" access="public"><cfset var local = 1></cffunction>
<cffunction name="hidden" access="private"></cffunction>
</cfcomponent>"#;
    let r = TreeSitterExtractor::new("Service.cfc", source, None).extract();
    let class = r
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Class)
        .expect("component declaration");
    assert_eq!(class.name, "Service");
    assert_eq!((class.start_line, class.end_line), (1, 4));
    assert_eq!(class.is_exported, Some(true));
    let greet = r.nodes.iter().find(|n| n.name == "greet").unwrap();
    assert_eq!(greet.kind, NodeKind::Method);
    assert_eq!(greet.qualified_name, "Service.cfc::Service::greet");
    assert_eq!((greet.start_line, greet.end_line), (2, 2));
    assert_eq!(greet.is_exported, Some(true));
    assert_eq!(
        r.nodes
            .iter()
            .find(|n| n.name == "hidden")
            .unwrap()
            .is_exported,
        Some(false)
    );
    assert!(
        r.edges
            .iter()
            .any(|e| e.source == class.id && e.target == greet.id && e.kind == EdgeKind::Contains)
    );
    assert!(!r.nodes.iter().any(|n| n.name == "local"));
}
#[test]
fn case_172_script_and_embedded_declarations() {
    for source in [
        "component {\n public string function greet() { var local = 1; return '你好'; }\n private function hidden() {}\n}",
        "<cfcomponent>\n<cfscript>\n public string function greet() { var local = 1; return '你好'; }\n private function hidden() {}\n</cfscript>\n</cfcomponent>",
    ] {
        let r = TreeSitterExtractor::new("Service.cfc", source, None).extract();
        assert!(r.errors.is_empty());
        assert_eq!(
            r.nodes.iter().filter(|n| n.kind == NodeKind::Class).count(),
            1
        );
        let greet = r
            .nodes
            .iter()
            .find(|n| n.name == "greet")
            .expect("script declaration");
        assert_eq!(greet.kind, NodeKind::Method);
        assert_eq!(greet.qualified_name, "Service.cfc::Service::greet");
        assert_eq!(greet.is_exported, Some(true));
        let row = if source.starts_with('<') { 3 } else { 2 };
        assert_eq!((greet.start_line, greet.end_line), (row, row));
        assert_eq!(
            r.nodes
                .iter()
                .find(|n| n.name == "hidden")
                .unwrap()
                .is_exported,
            Some(false)
        );
        assert!(!r.nodes.iter().any(|n| n.name == "local"));
    }
}
#[test]
fn case_172_negative_and_repeat_sqlite() {
    let dir = std::env::temp_dir().join(format!("rcg-172-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.as_path().join("page.cfm"),"<!--- <cffunction name='fake'></cffunction> --->\n<cfoutput>function fake() {}</cfoutput>\n<cffunction name='real'></cffunction>").unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(dir.as_path()).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let first = graph.get_nodes_in_file("page.cfm");
    assert_eq!(
        first
            .iter()
            .filter(|n| n.kind == NodeKind::Function && n.name == "real")
            .count(),
        1
    );
    assert!(!first.iter().any(|n| n.name == "fake"));
    assert!(graph.index_all(Default::default()).success);
    assert_eq!(graph.get_nodes_in_file("page.cfm").len(), first.len());
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_172_top_level_and_dynamic_names() {
    for source in [
        "function top() {}",
        "/* heading */\n function top() {}",
        "<cfscript>function top() {}</cfscript>",
        "<cffunction NAME='top'></cffunction>",
    ] {
        let r = TreeSitterExtractor::new("page.cfm", source, None).extract();
        let top = r
            .nodes
            .iter()
            .find(|n| n.name == "top")
            .expect("template function");
        assert_eq!(top.kind, NodeKind::Function);
        assert_eq!(top.qualified_name, "page.cfm::top");
    }
    for source in [
        "<cffunction></cffunction>",
        "<cffunction name='#dynamic#'></cffunction>",
        "<cfscript>var f = function() {};</cfscript>",
        "component { function broken( {",
    ] {
        let r = TreeSitterExtractor::new("page.cfm", source, None).extract();
        assert!(
            !r.nodes
                .iter()
                .any(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method)),
            "{source}: {:?}",
            r.nodes
        );
    }
}
