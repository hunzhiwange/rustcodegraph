use super::*;

mod nestjs_resolver_post_extract_router_module {
    use super::*;

    fn make_context(files: &[(&str, &str)], nodes: Vec<Node>) -> MockResolutionContext {
        let all_files = files.iter().map(|(path, _)| *path).collect::<Vec<_>>();
        MockResolutionContext::with_nodes(nodes)
            .with_file_contents(files)
            .with_all_files(&all_files)
    }

    // describe('nestjsResolver.postExtract - RouterModule')
    // it('prepends RouterModule prefix to a controller route (top-level register)')
    #[test]
    fn prepends_router_module_prefix_to_a_controller_route_top_level_register() {
        let mut ctx = make_context(
            &[(
                "src/app.module.ts",
                r#"
          @Module({
            imports: [
              RouterModule.register([
                { path: 'admin', module: AdminModule },
              ]),
            ],
          })
          export class AppModule {}

          @Module({ controllers: [AdminController] })
          export class AdminModule {}
        "#,
            )],
            vec![
                mk_class("AdminController", "src/admin/admin.controller.ts", 1, 10),
                mk_route("src/admin/admin.controller.ts", 3, "GET", "/", None),
            ],
        );

        let updates = NESTJS_RESOLVER.post_extract(&mut ctx);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "GET /admin");
        assert_eq!(updates[0].id, "route:src/admin/admin.controller.ts:3:GET:/");
        assert_eq!(
            updates[0].qualified_name,
            "src/admin/admin.controller.ts::GET:/"
        );
    }

    // it('resolves nested children - the issue #459 example')
    #[test]
    fn resolves_nested_children_the_issue_459_example() {
        let mut ctx = make_context(
            &[
                (
                    "src/app.module.ts",
                    r#"
          @Module({
            imports: [
              AdminModule,
              UsersModule,
              RouterModule.register([
                {
                  path: 'admin',
                  module: AdminModule,
                  children: [
                    { path: 'users', module: UsersModule },
                  ],
                },
              ]),
            ],
          })
          export class AppModule {}
        "#,
                ),
                (
                    "src/users/users.module.ts",
                    r#"
          @Module({ controllers: [UsersController] })
          export class UsersModule {}
        "#,
                ),
            ],
            vec![
                mk_class("UsersController", "src/users/users.controller.ts", 1, 10),
                mk_route("src/users/users.controller.ts", 3, "GET", "/", None),
            ],
        );

        let updates = NESTJS_RESOLVER.post_extract(&mut ctx);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "GET /admin/users");
    }

    // it('joins module prefix with a non-empty @Controller path and method params')
    #[test]
    fn joins_module_prefix_with_a_non_empty_controller_path_and_method_params() {
        let mut ctx = make_context(
            &[(
                "src/app.module.ts",
                r#"
          RouterModule.register([{ path: 'admin', module: UsersModule }])

          @Module({ controllers: [UsersController] })
          export class UsersModule {}
        "#,
            )],
            vec![
                mk_class("UsersController", "src/users.controller.ts", 1, 10),
                mk_route("src/users.controller.ts", 3, "GET", "/users/:id", None),
            ],
        );

        let updates = NESTJS_RESOLVER.post_extract(&mut ctx);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "GET /admin/users/:id");
    }

    // it('is idempotent - a second run returns no updates')
    #[test]
    fn is_idempotent_a_second_run_returns_no_updates() {
        let mut ctx = make_context(
            &[(
                "src/app.module.ts",
                r#"
          RouterModule.register([{ path: 'admin', module: UsersModule }])
          @Module({ controllers: [UsersController] })
          export class UsersModule {}
        "#,
            )],
            vec![
                mk_class("UsersController", "src/users.controller.ts", 1, 10),
                mk_route("src/users.controller.ts", 3, "GET", "/", Some("GET /admin")),
            ],
        );

        let updates = NESTJS_RESOLVER.post_extract(&mut ctx);
        assert!(updates.is_empty());
    }

    // it('is a no-op when the project does not use RouterModule')
    #[test]
    fn is_a_no_op_when_the_project_does_not_use_router_module() {
        let mut ctx = make_context(
            &[(
                "src/app.module.ts",
                r#"
          @Module({ controllers: [UsersController] })
          export class AppModule {}
        "#,
            )],
            vec![
                mk_class("UsersController", "src/users.controller.ts", 1, 10),
                mk_route("src/users.controller.ts", 3, "GET", "/", None),
            ],
        );

        let updates = NESTJS_RESOLVER.post_extract(&mut ctx);
        assert!(updates.is_empty());
    }

    // it('attributes routes to the right controller when one file has two')
    #[test]
    fn attributes_routes_to_the_right_controller_when_one_file_has_two() {
        let mut ctx = make_context(
            &[(
                "src/app.module.ts",
                r#"
          RouterModule.register([
            { path: 'p1', module: AModule },
            { path: 'p2', module: BModule },
          ])
          @Module({ controllers: [AController] }) export class AModule {}
          @Module({ controllers: [BController] }) export class BModule {}
        "#,
            )],
            vec![
                mk_class("AController", "src/multi.controller.ts", 1, 5),
                mk_class("BController", "src/multi.controller.ts", 7, 12),
                mk_route("src/multi.controller.ts", 3, "GET", "/a/x", None),
                mk_route("src/multi.controller.ts", 9, "GET", "/b/y", None),
            ],
        );

        let updates = NESTJS_RESOLVER.post_extract(&mut ctx);
        assert_eq!(updates.len(), 2);
        let by_id = updates
            .iter()
            .map(|node| (node.id.as_str(), node.name.as_str()))
            .collect::<HashMap<_, _>>();
        assert_eq!(
            by_id.get("route:src/multi.controller.ts:3:GET:/a/x"),
            Some(&"GET /p1/a/x")
        );
        assert_eq!(
            by_id.get("route:src/multi.controller.ts:9:GET:/b/y"),
            Some(&"GET /p2/b/y")
        );
    }

    // it('merges RouterModule registrations spread across multiple module files')
    #[test]
    fn merges_router_module_registrations_spread_across_multiple_module_files() {
        let mut ctx = make_context(
            &[
                (
                    "src/app.module.ts",
                    r#"
          RouterModule.register([{ path: 'a', module: AModule }])
          @Module({ controllers: [AController] }) export class AModule {}
        "#,
                ),
                (
                    "src/feature.module.ts",
                    r#"
          RouterModule.forChild([{ path: 'b', module: BModule }])
          @Module({ controllers: [BController] }) export class BModule {}
        "#,
                ),
            ],
            vec![
                mk_class("AController", "src/a.controller.ts", 1, 5),
                mk_class("BController", "src/b.controller.ts", 1, 5),
                mk_route("src/a.controller.ts", 3, "GET", "/", None),
                mk_route("src/b.controller.ts", 3, "GET", "/", None),
            ],
        );

        let updates = NESTJS_RESOLVER.post_extract(&mut ctx);
        assert_eq!(updates.len(), 2);
        let by_id = updates
            .iter()
            .map(|node| (node.id.as_str(), node.name.as_str()))
            .collect::<HashMap<_, _>>();
        assert_eq!(
            by_id.get("route:src/a.controller.ts:3:GET:/"),
            Some(&"GET /a")
        );
        assert_eq!(
            by_id.get("route:src/b.controller.ts:3:GET:/"),
            Some(&"GET /b")
        );
    }

    // it('silently skips controllers whose class node is not in the graph')
    #[test]
    fn silently_skips_controllers_whose_class_node_is_not_in_the_graph() {
        let mut ctx = make_context(
            &[(
                "src/app.module.ts",
                r#"
          RouterModule.register([{ path: 'orphans', module: GhostModule }])
          @Module({ controllers: [GhostController] }) export class GhostModule {}
        "#,
            )],
            vec![],
        );

        let updates = NESTJS_RESOLVER.post_extract(&mut ctx);
        assert!(updates.is_empty());
    }
}
