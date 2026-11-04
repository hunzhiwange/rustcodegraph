use rustcodegraph::extraction::tree_sitter::TreeSitterExtractor;
use rustcodegraph::types::ReferenceKind;

#[test]
fn case_173_script_references() {
    let source = "component extends='base.Parent' { function run() { var text='你好'; new app.Service(); helper(); createObject('component','app.Other'); include 'parts/view.cfm'; function nested() { new app.Inner(); } } }";
    let r = TreeSitterExtractor::new("Service.cfc", source, None).extract();
    for (name, kind, owner) in [
        ("helper", ReferenceKind::Calls, "run"),
        ("base.Parent", ReferenceKind::Extends, "Service"),
        ("app.Service", ReferenceKind::Instantiates, "run"),
        ("app.Other", ReferenceKind::Instantiates, "run"),
        ("parts/view.cfm", ReferenceKind::Imports, "run"),
        ("app.Inner", ReferenceKind::Instantiates, "nested"),
    ] {
        let reference = r
            .unresolved_references
            .iter()
            .find(|v| v.reference_name == name && v.reference_kind == kind)
            .unwrap_or_else(|| panic!("missing {name}: {:?}", r.unresolved_references));
        assert_eq!(
            r.nodes
                .iter()
                .find(|n| n.id == reference.from_node_id)
                .unwrap()
                .name,
            owner
        );
        if name == "app.Service" {
            assert_eq!(
                reference.column,
                source.find("new app.Service").unwrap() as u64
            );
        }
    }
}
#[test]
fn case_173_tags_and_embedded_script() {
    let source = "<cfcomponent extends='base.Parent'>\n<cffunction name='run'><cfinclude template='view.cfm'><cfscript>new app.Service();</cfscript></cffunction>\n</cfcomponent>";
    let r = TreeSitterExtractor::new("Service.cfc", source, None).extract();
    for name in ["base.Parent", "view.cfm", "app.Service"] {
        assert!(
            r.unresolved_references
                .iter()
                .any(|v| v.reference_name == name),
            "missing {name}: {:?}",
            r.unresolved_references
        );
    }
}
#[test]
fn case_173_dynamic_and_non_component_negatives() {
    for source in [
        "function run() { /* new fake.Comment(); */ var text=\"new fake.String()\"; createObject('java','java.lang.String'); createObject('component', path); new '#path#'(); include '#dir#/view.cfm'; }",
        "<!--- <cfinclude template='fake.cfm'> ---><cfinclude template='#dir#/view.cfm'>",
    ] {
        let r = TreeSitterExtractor::new("page.cfm", source, None).extract();
        assert!(
            r.unresolved_references.is_empty(),
            "{:?}",
            r.unresolved_references
        );
    }
}

#[test]
fn case_173_named_arguments_and_sqlite() {
    let source = "component { function run() { createObject(component='app.Named', type='component'); createObject('app.Single'); cfinclude(template='parts/named.cfm'); } }";
    let r = TreeSitterExtractor::new("Service.cfc", source, None).extract();
    for name in ["app.Named", "app.Single", "parts/named.cfm"] {
        assert!(
            r.unresolved_references
                .iter()
                .any(|v| v.reference_name == name),
            "missing {name}: {:?}",
            r.unresolved_references
        );
    }
    let dir = std::env::temp_dir().join(format!("rcg-173-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.as_path().join("Service.cfc"), source).unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(dir.as_path()).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let count = graph.get_nodes_in_file("Service.cfc").len();
    assert!(graph.index_all(Default::default()).success);
    assert_eq!(graph.get_nodes_in_file("Service.cfc").len(), count);
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
