mod describe_4205_python_absolute_module_import_resolution {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Python absolute module import resolution";
    const TS_DESCRIBE_LINE: usize = 4205;
    #[test]
    fn describes_060_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4205);
    }
    #[test]
    fn case_4218_links_a_bare_import_pkg_module_of_an_internal_module_to_its_file() {
        let suite = ["Python absolute module import resolution"];
        assert_eq!(suite.len(), 1);
        assert_eq!(228, 228);
        let temp = TempDir::new("codegraph-python-absolute-import");
        temp.write("conduit/__init__.py", "");
        temp.write("conduit/apps/__init__.py", "");
        temp.write(
            "conduit/apps/signals.py",
            r#"def handler():
    pass
"#,
        );
        temp.write(
            "conduit/apps/app.py",
            r#"import conduit.apps.signals
import os

VALUE = 1
"#,
        );

        let mut cg = index_project(&temp);
        let signals = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("conduit/apps/signals.py"))
            .expect("signals.py file should be indexed");
        let deps = impact_file_paths(&mut cg, &signals.id, 2);
        assert!(
            deps.iter().any(|path| path.ends_with("app.py")),
            "bare dotted import should reach app.py: {deps:?}"
        );
        let os_node = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("/os.py"));
        assert!(os_node.is_none(), "stdlib os.py should not be fabricated");
        cg.close();
    }
    #[test]
    fn case_4245_django_include_links_the_root_urlconf_to_the_included_app_urls_module() {
        let suite = ["Python absolute module import resolution"];
        assert_eq!(suite.len(), 1);
        assert_eq!(229, 229);
        let temp = TempDir::new("codegraph-python-django-include");
        temp.write("requirements.txt", "django==4.0\n");
        temp.write("app/__init__.py", "");
        temp.write(
            "app/views.py",
            r#"def home(request):
    return None
"#,
        );
        temp.write(
            "app/urls.py",
            r#"from django.conf.urls import url
from . import views
urlpatterns = [url(r'^$', views.home)]
"#,
        );
        temp.write(
            "urls.py",
            r#"from django.conf.urls import include, url
urlpatterns = [url(r'^app/', include('app.urls'))]
"#,
        );

        let mut cg = index_project(&temp);
        let app_urls = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("app/urls.py"))
            .expect("app/urls.py should be indexed");
        let deps = impact_file_paths(&mut cg, &app_urls.id, 2);
        assert!(
            deps.iter()
                .any(|path| path.ends_with("urls.py") && !path.ends_with("app/urls.py")),
            "root urlconf should depend on included app urls: {deps:?}"
        );
        cg.close();
    }
    #[test]
    fn case_4272_resolves_from_pkg_import_submodule_to_the_submodule_under_that_package() {
        let suite = ["Python absolute module import resolution"];
        assert_eq!(suite.len(), 1);
        assert_eq!(230, 230);
        let temp = TempDir::new("codegraph-python-from-package-import");
        for path in [
            "app/__init__.py",
            "app/api/__init__.py",
            "app/api/routes/__init__.py",
            "app/api/dependencies/__init__.py",
        ] {
            temp.write(path, "");
        }
        temp.write(
            "app/api/routes/authentication.py",
            r#"def login():
    pass
"#,
        );
        temp.write(
            "app/api/dependencies/authentication.py",
            r#"def get_user():
    pass
"#,
        );
        temp.write(
            "app/api/routes/api.py",
            r#"from app.api.routes import authentication

ROUTER = authentication
"#,
        );

        let mut cg = index_project(&temp);
        let routes_auth = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("routes/authentication.py"))
            .expect("routes/authentication.py should be indexed");
        let deps_auth = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("dependencies/authentication.py"))
            .expect("dependencies/authentication.py should be indexed");
        let routes_deps = impact_file_paths(&mut cg, &routes_auth.id, 2);
        let deps_deps = impact_file_paths(&mut cg, &deps_auth.id, 2);
        assert!(
            routes_deps
                .iter()
                .any(|path| path.ends_with("routes/api.py")),
            "package submodule import should reach routes/api.py: {routes_deps:?}"
        );
        assert!(
            deps_deps
                .iter()
                .all(|path| !path.ends_with("routes/api.py")),
            "sibling same-named module should not reach routes/api.py: {deps_deps:?}"
        );
        cg.close();
    }
}
