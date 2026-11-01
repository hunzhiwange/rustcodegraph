use rustcodegraph::types::{EdgeKind, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};

#[test]
fn case_020_signature_types() {
    let dir = std::env::temp_dir().join(format!("rcg20-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.as_path().join("sample.go"),
        r#"package sample
import "net/http"
import "crypto/tls"
type Wanted interface { Match(*http.Request) bool }
type Good struct{}
func (g *Good) Match(r *http.Request) bool { return true }
type RequestAlias = http.Request
type AliasGood struct{}
func (g *AliasGood) Match(r *RequestAlias) bool { return true }
type BadParam struct{}
func (g BadParam) Match(r *tls.ClientHelloInfo) bool { return true }
var selected Wanted
func wire() { selected = BadParam{} }
type BadResult struct{}
func (g BadResult) Match(r *http.Request) string { return "" }
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("other.go"),
        r#"package sample
import h "net/http"
import other "example/http"
type ImportedGood struct{}
func (g ImportedGood) Match(r *h.Request) bool { return true }
type BadPackage struct{}
func (g BadPackage) Match(r *other.Request) bool { return true }
"#,
    )
    .unwrap();
    let mut cg = CodeGraph::init_sync(dir.as_path()).unwrap();
    let result = cg.index_all(IndexOptions::default());
    assert!(result.success, "{:?}", result.errors);
    let iface = cg
        .get_nodes_by_kind(NodeKind::Interface)
        .into_iter()
        .find(|n| n.name == "Wanted")
        .unwrap();
    for node in cg.get_nodes_by_kind(NodeKind::Struct) {
        let matches = cg
            .get_outgoing_edges(&node.id)
            .iter()
            .any(|e| e.kind == EdgeKind::Implements && e.target == iface.id);
        assert_eq!(
            matches,
            matches!(node.name.as_str(), "Good" | "AliasGood" | "ImportedGood"),
            "{}",
            node.name
        );
    }
    assert!(!cg.get_outgoing_edges(&iface.id).iter().any(|e| {
        e.metadata
            .as_ref()
            .and_then(|m| m.get("synthesizedBy"))
            .and_then(|v| v.as_str())
            == Some("go-interface-assignment")
    }));
    cg.close();
    std::fs::remove_dir_all(dir).unwrap();
}
