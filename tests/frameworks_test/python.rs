use super::*;

mod django_resolver_extract {
    use super::*;

    // describe('djangoResolver.extract')
    // it('extracts route node and reference for path() with CBV.as_view()')
    #[test]
    fn extracts_route_node_and_reference_for_path_with_cbv_as_view() {
        let src = r#"
from django.urls import path
from users.views import UserListView

urlpatterns = [
    path('users/', UserListView.as_view(), name='user-list'),
]
"#;
        let FrameworkExtractionResult { nodes, references } =
            DJANGO_RESOLVER.extract("users/urls.py", src);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, NodeKind::Route);
        assert_eq!(nodes[0].name, "users/");
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].reference_name, "UserListView");
        assert_eq!(references[0].reference_kind, ReferenceKind::References);
        assert_eq!(references[0].from_node_id, nodes[0].id);
    }

    // it('extracts route for path() with dotted module.Class.as_view()')
    #[test]
    fn extracts_route_for_path_with_dotted_module_class_as_view() {
        let src = "from django.urls import path\nfrom api.v1 import views as api_v1_views\nurlpatterns = [path('api/', api_v1_views.UserListView.as_view())]\n";
        let result = DJANGO_RESOLVER.extract("api/urls.py", src);
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.references[0].reference_name, "UserListView");
    }

    // it('extracts route for path() with bare function view')
    #[test]
    fn extracts_route_for_path_with_bare_function_view() {
        let src =
            "from django.urls import path\nurlpatterns = [path('home/', home_view, name='home')]\n";
        let result = DJANGO_RESOLVER.extract("home/urls.py", src);
        assert_eq!(result.references[0].reference_name, "home_view");
    }

    // it('extracts route for path() with include()')
    #[test]
    fn extracts_route_for_path_with_include() {
        let src = "from django.urls import path, include\nurlpatterns = [path('api/', include('api.urls'))]\n";
        let result = DJANGO_RESOLVER.extract("root/urls.py", src);
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].kind, NodeKind::Route);
        assert_eq!(result.references[0].reference_name, "api.urls");
        assert_eq!(result.references[0].reference_kind, ReferenceKind::Imports);
    }

    // it('extracts routes for re_path and url')
    #[test]
    fn extracts_routes_for_re_path_and_url() {
        let src = "from django.urls import re_path, url\nurlpatterns = [re_path(r'^users/$', UserView), url(r'^old/$', OldView)]\n";
        let result = DJANGO_RESOLVER.extract("legacy/urls.py", src);
        assert_eq!(result.nodes.len(), 2);
        assert_eq!(names(&result.nodes), vec!["^users/$", "^old/$"]);
    }

    // it('returns empty result for a non-urls.py python file')
    #[test]
    fn returns_empty_result_for_a_non_urls_py_python_file() {
        let result = DJANGO_RESOLVER.extract("views.py", "def foo(): return 1\n");
        assert!(result.nodes.is_empty());
        assert!(result.references.is_empty());
    }
}

mod flask_resolver_extract {
    use super::*;

    // describe('flaskResolver.extract')
    // it('extracts route and reference from @app.route')
    #[test]
    fn extracts_route_and_reference_from_app_route() {
        let src = r#"
@app.route('/users')
def list_users():
    return []
"#;
        let result = FLASK_RESOLVER.extract("app.py", src);
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].kind, NodeKind::Route);
        assert_eq!(result.nodes[0].name, "GET /users");
        assert_eq!(result.references[0].reference_name, "list_users");
    }

    // it('extracts blueprint routes')
    #[test]
    fn extracts_blueprint_routes() {
        let src = r#"
@users_bp.route('/<id>', methods=['POST'])
def create_user(id):
    pass
"#;
        let result = FLASK_RESOLVER.extract("routes.py", src);
        assert_eq!(result.nodes[0].name, "POST /<id>");
        assert_eq!(result.references[0].reference_name, "create_user");
    }

    // it('resolves the handler across an intervening decorator (@login_required)')
    #[test]
    fn resolves_the_handler_across_an_intervening_decorator() {
        let src = r#"
@bp.route('/profile')
@login_required
def profile():
    return render_template('profile.html')
"#;
        let result = FLASK_RESOLVER.extract("routes.py", src);
        assert_eq!(result.nodes[0].name, "GET /profile");
        assert_eq!(result.references[0].reference_name, "profile");
    }

    // it('extracts stacked @x.route decorators bound to one view')
    #[test]
    fn extracts_stacked_x_route_decorators_bound_to_one_view() {
        let src = r#"
@bp.route('/', methods=['GET', 'POST'])
@bp.route('/index', methods=['GET', 'POST'])
@login_required
def index():
    return render_template('index.html')
"#;
        let result = FLASK_RESOLVER.extract("routes.py", src);
        assert_eq!(names(&result.nodes), vec!["GET /", "GET /index"]);
        assert_eq!(reference_names(&result.references), vec!["index", "index"]);
    }

    // it('extracts the method from a tuple methods=(...) (not just a list)')
    #[test]
    fn extracts_the_method_from_a_tuple_methods_not_just_a_list() {
        let src = r#"
@blueprint.route('/api/articles', methods=('POST',))
def make_article():
    pass
"#;
        let result = FLASK_RESOLVER.extract("views.py", src);
        assert_eq!(result.nodes[0].name, "POST /api/articles");
        assert_eq!(result.references[0].reference_name, "make_article");
    }

    // it('extracts Flask-RESTful api.add_resource(Resource, paths) -> the Resource class')
    #[test]
    fn extracts_flask_restful_add_resource_resource_paths_to_resource_class() {
        let src = r#"
api.add_resource(TodoResource, '/todos/<id>')
api.add_org_resource(AlertResource, '/api/alerts/<id>', endpoint='alert')
"#;
        let result = FLASK_RESOLVER.extract("api.py", src);
        assert_eq!(
            names(&result.nodes),
            vec!["ANY /todos/<id>", "ANY /api/alerts/<id>"]
        );
        assert_eq!(
            reference_names(&result.references),
            vec!["TodoResource", "AlertResource"]
        );
    }
}

mod fastapi_resolver_extract {
    use super::*;

    // describe('fastapiResolver.extract')
    // it('extracts route and reference from @app.get')
    #[test]
    fn extracts_route_and_reference_from_app_get() {
        let src = r#"
@app.get('/users')
async def list_users():
    return []
"#;
        let result = FASTAPI_RESOLVER.extract("main.py", src);
        assert_eq!(result.nodes[0].name, "GET /users");
        assert_eq!(result.references[0].reference_name, "list_users");
    }

    // it('extracts route from router.post')
    #[test]
    fn extracts_route_from_router_post() {
        let src = r#"
@router.post('/items')
def create_item(item: Item):
    pass
"#;
        let result = FASTAPI_RESOLVER.extract("items.py", src);
        assert_eq!(result.nodes[0].name, "POST /items");
        assert_eq!(result.references[0].reference_name, "create_item");
    }

    // it('extracts a route mounted at the router/prefix root (empty path)')
    #[test]
    fn extracts_a_route_mounted_at_the_router_prefix_root_empty_path() {
        let src = r#"
@router.get("", response_model=ListOfArticles, name="articles:list")
async def list_articles():
    return []
"#;
        let result = FASTAPI_RESOLVER.extract("articles.py", src);
        assert_eq!(result.nodes[0].name, "GET /");
        assert_eq!(result.references[0].reference_name, "list_articles");
    }

    // it('extracts a multi-line decorator with an empty path')
    #[test]
    fn extracts_a_multi_line_decorator_with_an_empty_path() {
        let src = r#"
@router.post(
    "",
    status_code=201,
    response_model=ArticleInResponse,
)
async def create_article():
    pass
"#;
        let result = FASTAPI_RESOLVER.extract("articles.py", src);
        assert_eq!(result.nodes[0].name, "POST /");
        assert_eq!(result.references[0].reference_name, "create_article");
    }
}
