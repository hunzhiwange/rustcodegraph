use super::*;

mod go_resolver_extract {
    use super::*;

    // describe('goResolver.extract')
    // it('extracts route from r.GET')
    #[test]
    fn extracts_route_from_r_get() {
        let result = GO_RESOLVER.extract("main.go", "r.GET(\"/users\", listUsers)\n");
        assert_eq!(result.nodes[0].name, "GET /users");
        assert_eq!(result.references[0].reference_name, "listUsers");
    }

    // it('extracts route from router.HandleFunc')
    #[test]
    fn extracts_route_from_router_handle_func() {
        let result = GO_RESOLVER.extract("main.go", "router.HandleFunc(\"/items\", createItem)\n");
        assert_eq!(result.references[0].reference_name, "createItem");
    }

    // it('extracts gorilla/mux HandleFunc on a subrouter var, ignoring chained .Methods()')
    #[test]
    fn extracts_gorilla_mux_handle_func_on_a_subrouter_var_ignoring_chained_methods() {
        let result = GO_RESOLVER.extract(
            "routes.go",
            "s.HandleFunc(\"/users/{id}\", listUsers).Methods(\"GET\")\n",
        );
        assert_eq!(result.references[0].reference_name, "listUsers");
    }
}

mod rust_resolver_extract {
    use super::*;

    // describe('rustResolver.extract')
    // it('extracts route from axum .route with get()')
    #[test]
    fn extracts_route_from_axum_route_with_get() {
        let result = RUST_RESOLVER.extract(
            "main.rs",
            "let app = Router::new().route(\"/users\", get(list_users));\n",
        );
        assert_eq!(result.nodes[0].name, "GET /users");
        assert_eq!(result.references[0].reference_name, "list_users");
    }

    // it('extracts every method from a chained axum .route (get().put())')
    #[test]
    fn extracts_every_method_from_a_chained_axum_route_get_put() {
        let result = RUST_RESOLVER.extract(
            "main.rs",
            "let app = Router::new().route(\"/user\", get(get_current_user).put(update_user));\n",
        );
        assert_eq!(names(&result.nodes), vec!["GET /user", "PUT /user"]);
        assert_eq!(
            reference_names(&result.references),
            vec!["get_current_user", "update_user"]
        );
    }

    // it('extracts a multi-line axum .route with a namespaced handler')
    #[test]
    fn extracts_a_multi_line_axum_route_with_a_namespaced_handler() {
        let src = r#"
let app = Router::new()
    .route(
        "/articles/feed",
        get(listing::feed_articles),
    );
"#;
        let result = RUST_RESOLVER.extract("main.rs", src);
        assert_eq!(result.nodes[0].name, "GET /articles/feed");
        assert_eq!(result.references[0].reference_name, "feed_articles");
    }

    // it('extracts actix web::resource().route(web::METHOD().to(handler))')
    #[test]
    fn extracts_actix_web_resource_route_web_method_to_handler() {
        let result = RUST_RESOLVER.extract(
            "main.rs",
            "App::new().service(web::resource(\"/user/{id}\").route(web::get().to(get_user)))\n",
        );
        assert_eq!(result.nodes[0].name, "GET /user/{id}");
        assert_eq!(result.references[0].reference_name, "get_user");
    }

    // it('extracts actix web::resource(\"/\").to(handler) (all methods)')
    #[test]
    fn extracts_actix_web_resource_to_handler_all_methods() {
        let result = RUST_RESOLVER.extract(
            "main.rs",
            "App::new().service(web::resource(\"/\").to(index))\n",
        );
        assert_eq!(result.nodes[0].name, "ANY /");
        assert_eq!(result.references[0].reference_name, "index");
    }

    // it('extracts actix App-level .route(\"/path\", web::METHOD().to(handler))')
    #[test]
    fn extracts_actix_app_level_route_path_web_method_to_handler() {
        let result = RUST_RESOLVER.extract(
            "main.rs",
            "App::new().route(\"/health\", web::get().to(health_check))\n",
        );
        assert_eq!(result.nodes[0].name, "GET /health");
        assert_eq!(result.references[0].reference_name, "health_check");
    }
}

mod rust_resolver_resolve_cargo_workspace_crates {
    use super::*;

    fn rust_module(id: &str, name: &str, file_path: &str) -> Node {
        node(
            id,
            NodeKind::Module,
            name,
            &format!("{file_path}::{name}"),
            file_path,
            Language::Rust,
            1,
            1,
        )
    }

    // describe('rustResolver.resolve cargo workspace crates')
    // it('resolves crate name from workspace member lib.rs')
    #[test]
    fn resolves_crate_name_from_workspace_member_lib_rs() {
        let lib_node = rust_module(
            "module:crates/mytool-core/src/lib.rs:mytool_core:1",
            "mytool_core",
            "crates/mytool-core/src/lib.rs",
        );
        let mut context = MockResolutionContext::with_nodes(vec![lib_node.clone()])
            .with_file_contents(&[
                (
                    "Cargo.toml",
                    r#"
[workspace]
members = ["crates/mytool-core", "crates/mytool-fetcher"]
"#,
                ),
                (
                    "crates/mytool-core/Cargo.toml",
                    r#"
[package]
name = "mytool-core"
version = "0.1.0"
"#,
                ),
            ])
            .with_files(&["crates/mytool-core/src/lib.rs"])
            .with_all_files(&[
                "Cargo.toml",
                "crates/mytool-core/Cargo.toml",
                "crates/mytool-core/src/lib.rs",
            ]);
        let reference = unresolved_ref(
            "fn:crates/mytool-fetcher/src/main.rs:main:1",
            "mytool_core",
            ReferenceKind::References,
            "crates/mytool-fetcher/src/main.rs",
            Language::Rust,
        );
        let result = RUST_RESOLVER
            .resolve(&reference, &mut context)
            .expect("workspace lib crate should resolve");
        assert_eq!(result.target_node_id, lib_node.id);
        assert_eq!(result.resolved_by, ResolvedBy::Framework);
        assert!(result.confidence >= 0.9);
    }

    // it('resolves crate name from workspace member main.rs when lib.rs is absent')
    #[test]
    fn resolves_crate_name_from_workspace_member_main_rs_when_lib_rs_is_absent() {
        let main_node = rust_module(
            "module:crates/mytool-runner/src/main.rs:mytool_runner:1",
            "mytool_runner",
            "crates/mytool-runner/src/main.rs",
        );
        let mut context = MockResolutionContext::with_nodes(vec![main_node.clone()])
            .with_file_contents(&[
                (
                    "Cargo.toml",
                    r#"
[workspace]
members = [
  "crates/mytool-runner",
]
"#,
                ),
                (
                    "crates/mytool-runner/Cargo.toml",
                    r#"
[package]
name = "mytool-runner"
version = "0.1.0"
"#,
                ),
            ])
            .with_files(&["crates/mytool-runner/src/main.rs"])
            .with_all_files(&[
                "Cargo.toml",
                "crates/mytool-runner/Cargo.toml",
                "crates/mytool-runner/src/main.rs",
            ]);
        let reference = unresolved_ref(
            "fn:crates/mytool-runner/src/main.rs:main:1",
            "mytool_runner",
            ReferenceKind::References,
            "crates/mytool-runner/src/main.rs",
            Language::Rust,
        );
        let result = RUST_RESOLVER
            .resolve(&reference, &mut context)
            .expect("workspace bin crate should resolve");
        assert_eq!(result.target_node_id, main_node.id);
        assert_eq!(result.resolved_by, ResolvedBy::Framework);
    }

    // it('resolves crate name when members uses a glob (crates/*)')
    #[test]
    fn resolves_crate_name_when_members_uses_a_glob_crates_star() {
        let foo_lib = rust_module(
            "module:crates/mytool-foo/src/lib.rs:mytool_foo:1",
            "mytool_foo",
            "crates/mytool-foo/src/lib.rs",
        );
        let bar_lib = rust_module(
            "module:crates/mytool-bar/src/lib.rs:mytool_bar:1",
            "mytool_bar",
            "crates/mytool-bar/src/lib.rs",
        );
        let mut context = cargo_workspace_context(
            &[
                (
                    "Cargo.toml",
                    r#"
[workspace]
members = ["crates/*"]
"#,
                ),
                (
                    "crates/mytool-foo/Cargo.toml",
                    r#"
[package]
name = "mytool-foo"
version = "0.1.0"
"#,
                ),
                (
                    "crates/mytool-bar/Cargo.toml",
                    r#"
[package]
name = "mytool-bar"
version = "0.1.0"
"#,
                ),
            ],
            &[
                ("crates/mytool-foo/src/lib.rs", vec![foo_lib.clone()]),
                ("crates/mytool-bar/src/lib.rs", vec![bar_lib.clone()]),
            ],
            &[
                (".", &["crates"]),
                ("crates", &["mytool-foo", "mytool-bar"]),
                ("crates/mytool-foo", &["src"]),
                ("crates/mytool-bar", &["src"]),
            ],
        );
        let foo_ref = unresolved_ref(
            "fn:crates/mytool-bar/src/lib.rs:other:1",
            "mytool_foo",
            ReferenceKind::References,
            "crates/mytool-bar/src/lib.rs",
            Language::Rust,
        );
        let bar_ref = unresolved_ref(
            "fn:crates/mytool-foo/src/lib.rs:other:1",
            "mytool_bar",
            ReferenceKind::References,
            "crates/mytool-foo/src/lib.rs",
            Language::Rust,
        );
        assert_eq!(
            RUST_RESOLVER
                .resolve(&foo_ref, &mut context)
                .expect("foo crate should resolve")
                .target_node_id,
            foo_lib.id
        );
        assert_eq!(
            RUST_RESOLVER
                .resolve(&bar_ref, &mut context)
                .expect("bar crate should resolve")
                .target_node_id,
            bar_lib.id
        );
    }

    // it('resolves crate name when members uses a name glob at root (helix-*)')
    #[test]
    fn resolves_crate_name_when_members_uses_a_name_glob_at_root_helix_star() {
        let core_lib = rust_module(
            "module:helix-core/src/lib.rs:helix_core:1",
            "helix_core",
            "helix-core/src/lib.rs",
        );
        let mut context = cargo_workspace_context(
            &[
                (
                    "Cargo.toml",
                    r#"
[workspace]
members = ["helix-*"]
"#,
                ),
                (
                    "helix-core/Cargo.toml",
                    r#"
[package]
name = "helix-core"
version = "0.1.0"
"#,
                ),
            ],
            &[("helix-core/src/lib.rs", vec![core_lib.clone()])],
            &[
                (".", &["helix-core", "docs", "target"]),
                ("helix-core", &["src"]),
            ],
        );
        let reference = unresolved_ref(
            "fn:helix-core/src/lib.rs:other:1",
            "helix_core",
            ReferenceKind::References,
            "helix-core/src/lib.rs",
            Language::Rust,
        );
        assert_eq!(
            RUST_RESOLVER
                .resolve(&reference, &mut context)
                .expect("helix-core should resolve")
                .target_node_id,
            core_lib.id
        );
    }
}
