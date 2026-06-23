use super::*;

#[test]
fn creates_a_route_view_edge_from_urls_py_to_view_class() {
    let project = TempProject::new("cg-django");
    project.write("manage.py", "# marker\n");
    project.write("requirements.txt", "django==4.2\n");
    project.mkdir("users");
    project.write("users/__init__.py", "");
    project.write(
        "users/views.py",
        "class UserListView:\n    def get(self, request): pass\n",
    );
    project.write(
        "users/urls.py",
        "from django.urls import path\n\
         from users.views import UserListView\n\
         urlpatterns = [path(\"users/\", UserListView.as_view(), name=\"user-list\")]\n",
    );

    let mut cg = index(&project);

    let routes = cg.get_nodes_by_kind(NodeKind::Route);
    assert!(!routes.is_empty());
    let route = routes
        .iter()
        .find(|node| node.name == "users/")
        .expect("route users/ should be defined");

    let class_nodes = cg.get_nodes_by_kind(NodeKind::Class);
    let view = class_nodes
        .iter()
        .find(|node| node.name == "UserListView")
        .expect("UserListView should be defined");

    let edges = cg.get_outgoing_edges(&route.id);
    let to_view = edges
        .iter()
        .find(|edge| edge.target == view.id)
        .expect("route should reference UserListView");
    assert_eq!(to_view.kind, EdgeKind::References);

    cg.close();
}
