use super::*;

mod express_resolver_extract {
    use super::*;

    // describe('expressResolver.extract')
    // it('extracts route with inline handler reference')
    #[test]
    fn extracts_route_with_inline_handler_reference() {
        let result = EXPRESS_RESOLVER.extract("routes.ts", "app.get('/users', listUsers);\n");
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].name, "GET /users");
        assert_eq!(result.references[0].reference_name, "listUsers");
    }

    // it('extracts route with router.post and middleware chain')
    #[test]
    fn extracts_route_with_router_post_and_middleware_chain() {
        let result =
            EXPRESS_RESOLVER.extract("items.ts", "router.post('/items', auth, createItem);\n");
        assert_eq!(result.nodes[0].name, "POST /items");
        assert_eq!(result.references[0].reference_name, "createItem");
    }

    // it('extracts route with controller method reference')
    #[test]
    fn extracts_route_with_controller_method_reference() {
        let result = EXPRESS_RESOLVER.extract("routes.ts", "app.get('/x', userController.list);\n");
        assert_eq!(result.references[0].reference_name, "list");
    }
}

mod react_resolver_extract_react_router {
    use super::*;

    // describe('reactResolver.extract - React Router')
    // it('extracts a v6 <Route path element={<Comp/>}>')
    #[test]
    fn extracts_a_v6_route_path_element_comp() {
        let result = REACT_RESOLVER.extract(
            "App.tsx",
            r#"<Route path="/users" element={<UsersPage/>}/>"#,
        );
        let route = result
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Route)
            .expect("route should be extracted");
        assert_eq!(route.name, "/users");
        assert_eq!(result.references[0].reference_name, "UsersPage");
    }

    // it('extracts a v5 <Route path component={Comp}> with attributes in any order')
    #[test]
    fn extracts_a_v5_route_path_component_comp_with_attributes_in_any_order() {
        let result = REACT_RESOLVER.extract(
            "App.jsx",
            r#"<Route exact path="/login" component={Login} />"#,
        );
        let route = result
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Route)
            .expect("route should be extracted");
        assert_eq!(route.name, "/login");
        assert_eq!(result.references[0].reference_name, "Login");
    }

    // it('does not treat the <Routes> container as a route')
    #[test]
    fn does_not_treat_the_routes_container_as_a_route() {
        let result = REACT_RESOLVER.extract(
            "App.tsx",
            r#"<Routes><Route path="/x" element={<X/>}/></Routes>"#,
        );
        let routes = route_names(&result.nodes);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0], "/x");
    }

    // it('extracts createBrowserRouter object routes ({ path, element/Component })')
    #[test]
    fn extracts_create_browser_router_object_routes_path_element_component() {
        let src = r#"const router = createBrowserRouter([
      { path: "/dashboard", element: <Dashboard /> },
      { path: "/login", Component: Login },
    ]);"#;
        let result = REACT_RESOLVER.extract("router.tsx", src);
        assert_eq!(
            sorted(route_names(&result.nodes)),
            vec!["/dashboard", "/login"]
        );
        assert_eq!(
            sorted(reference_names(&result.references)),
            vec!["Dashboard", "Login"]
        );
    }

    // it('does not treat config files or a nextjs-pages dir as Next.js routes')
    #[test]
    fn does_not_treat_config_files_or_a_nextjs_pages_dir_as_nextjs_routes() {
        let cfg = REACT_RESOLVER.extract("apps/nextjs-pages/next.config.mjs", "export default {}");
        assert!(route_names(&cfg.nodes).is_empty());
        let vite = REACT_RESOLVER.extract("src/pages/vite.config.ts", "export default {}");
        assert!(route_names(&vite.nodes).is_empty());
        let page = REACT_RESOLVER.extract(
            "src/pages/about.tsx",
            "export default function About(){return null}",
        );
        assert_eq!(route_names(&page.nodes), vec!["/about"]);
    }
}

mod svelte_resolver_extract_smoke {
    use super::*;

    // describe('svelteResolver.extract (smoke)')
    // it('returns { nodes, references } shape')
    #[test]
    fn returns_nodes_references_shape() {
        let result = SVELTE_RESOLVER.extract("+page.svelte", "");
        let FrameworkExtractionResult { nodes, references } = result;
        drop((nodes, references));
    }
}

mod astro_resolver_extract_src_pages_file_based_routing {
    use super::*;

    fn route_names_for(file_path: &str) -> Vec<String> {
        route_names(&ASTRO_RESOLVER.extract(file_path, "").nodes)
    }

    // describe('astroResolver.extract - src/pages file-based routing')
    // it('maps index.astro to /')
    #[test]
    fn maps_index_astro_to_root() {
        assert_eq!(route_names_for("src/pages/index.astro"), vec!["/"]);
    }

    // it('maps nested index and plain pages')
    #[test]
    fn maps_nested_index_and_plain_pages() {
        assert_eq!(route_names_for("src/pages/blog/index.astro"), vec!["/blog"]);
        assert_eq!(route_names_for("src/pages/about.astro"), vec!["/about"]);
    }

    // it('converts [param] and [...rest] syntax')
    #[test]
    fn converts_param_and_rest_syntax() {
        assert_eq!(
            route_names_for("src/pages/blog/[slug].astro"),
            vec!["/blog/:slug"]
        );
        assert_eq!(route_names_for("src/pages/[...path].astro"), vec!["/*path"]);
    }

    // it('maps .ts endpoints under src/pages to routes')
    #[test]
    fn maps_ts_endpoints_under_src_pages_to_routes() {
        assert_eq!(
            route_names_for("src/pages/api/posts.ts"),
            vec!["/api/posts"]
        );
        assert_eq!(route_names_for("src/pages/rss.xml.js"), vec!["/rss.xml"]);
    }

    // it('excludes underscore-prefixed segments and config files')
    #[test]
    fn excludes_underscore_prefixed_segments_and_config_files() {
        assert!(route_names_for("src/pages/_partial.astro").is_empty());
        assert!(route_names_for("src/pages/blog/_components/Card.astro").is_empty());
        assert!(route_names_for("src/pages/vite.config.ts").is_empty());
    }

    // it('ignores .astro files outside src/pages')
    #[test]
    fn ignores_astro_files_outside_src_pages() {
        assert!(route_names_for("src/components/Button.astro").is_empty());
        assert!(route_names_for("docs/pages/guide.astro").is_empty());
    }
}

mod astro_resolver_resolve_astro_global_and_virtual_modules {
    use super::*;

    fn base_ref(reference_name: &str, reference_kind: ReferenceKind) -> UnresolvedRef {
        unresolved_ref(
            "component:a",
            reference_name,
            reference_kind,
            "src/pages/index.astro",
            Language::Astro,
        )
    }

    // describe('astroResolver.resolve - Astro global and virtual modules')
    // it('claims Astro.* global references as framework-provided')
    #[test]
    fn claims_astro_global_references_as_framework_provided() {
        let mut ctx = MockResolutionContext::new();
        let result = ASTRO_RESOLVER
            .resolve(
                &base_ref("Astro.props", ReferenceKind::References),
                &mut ctx,
            )
            .expect("Astro.props should resolve");
        assert_eq!(result.resolved_by, ResolvedBy::Framework);
        assert_eq!(result.confidence, 1.0);
    }

    // it('claims astro:content virtual module imports')
    #[test]
    fn claims_astro_content_virtual_module_imports() {
        let mut ctx = MockResolutionContext::new();
        let result = ASTRO_RESOLVER
            .resolve(&base_ref("astro:content", ReferenceKind::Imports), &mut ctx)
            .expect("astro:content should resolve");
        assert_eq!(result.resolved_by, ResolvedBy::Framework);
    }

    // it('leaves ordinary names alone')
    #[test]
    fn leaves_ordinary_names_alone() {
        let mut ctx = MockResolutionContext::new();
        let result = ASTRO_RESOLVER.resolve(&base_ref("astrolabe", ReferenceKind::Calls), &mut ctx);
        assert!(result.is_none());
    }
}
