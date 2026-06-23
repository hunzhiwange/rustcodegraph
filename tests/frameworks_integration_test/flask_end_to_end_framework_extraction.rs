use super::*;

#[test]
fn resolves_stacked_routes_across_login_required_to_a_view_named_after_a_builtin_index() {
    let project = TempProject::new("cg-flask");
    project.write("requirements.txt", "flask==3.0\n");
    project.write(
        "app.py",
        "from flask import Blueprint, render_template\n\
         from flask_login import login_required\n\
         bp = Blueprint(\"main\", __name__)\n\
         \n\
         @bp.route(\"/\", methods=[\"GET\", \"POST\"])\n\
         @bp.route(\"/index\", methods=[\"GET\", \"POST\"])\n\
         @login_required\n\
         def index():\n\
             return render_template(\"index.html\")\n",
    );

    let mut cg = index(&project);

    let routes = cg.get_nodes_by_kind(NodeKind::Route);
    let mut route_names = routes
        .iter()
        .map(|route| route.name.clone())
        .collect::<Vec<_>>();
    route_names.sort();
    assert_eq!(
        route_names,
        vec!["GET /".to_owned(), "GET /index".to_owned()]
    );

    let fn_node = cg
        .get_nodes_by_kind(NodeKind::Function)
        .into_iter()
        .find(|node| node.name == "index")
        .expect("index function should be defined");

    for route in routes {
        let edges = cg.get_outgoing_edges(&route.id);
        let to_view = edges
            .iter()
            .find(|edge| edge.target == fn_node.id && edge.kind == EdgeKind::References);
        assert!(
            to_view.is_some(),
            "route {} should resolve to index()",
            route.name
        );
    }

    cg.close();
}
