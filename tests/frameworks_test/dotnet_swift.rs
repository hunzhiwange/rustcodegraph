use super::*;

mod aspnet_resolver_extract {
    use super::*;

    // describe('aspnetResolver.extract')
    // it('extracts route from [HttpGet] attribute')
    #[test]
    fn extracts_route_from_http_get_attribute() {
        let src = r#"
[HttpGet("/users")]
public IActionResult ListUsers()
{
  return Ok();
}
"#;
        let result = ASPNET_RESOLVER.extract("UserController.cs", src);
        assert_eq!(result.nodes[0].name, "GET /users");
        assert_eq!(result.references[0].reference_name, "ListUsers");
    }
}

mod vapor_resolver_extract {
    use super::*;

    // describe('vaporResolver.extract')
    // it('extracts route from app.get with use:')
    #[test]
    fn extracts_route_from_app_get_with_use() {
        let result = VAPOR_RESOLVER.extract("routes.swift", "app.get(\"users\", use: listUsers)\n");
        assert_eq!(result.nodes[0].name, "GET /users");
        assert_eq!(result.references[0].reference_name, "listUsers");
    }

    // it('extracts grouped RouteCollection routes with the group prefix and no path arg')
    #[test]
    fn extracts_grouped_route_collection_routes_with_group_prefix_and_no_path_arg() {
        let src = r#"
func boot(routes: RoutesBuilder) throws {
    let todos = routes.grouped("todos")
    todos.get(use: index)
    todos.post(use: create)
    todos.group(":todoID") { todo in
        todo.delete(use: delete)
    }
}
"#;
        let result = VAPOR_RESOLVER.extract("TodoController.swift", src);
        assert_eq!(
            sorted(names(&result.nodes)),
            vec!["DELETE /todos/:todoID", "GET /todos", "POST /todos"]
        );
        assert_eq!(
            sorted(reference_names(&result.references)),
            vec!["create", "delete", "index"]
        );
    }

    // it('handles use: self.handler and non-string path segments')
    #[test]
    fn handles_use_self_handler_and_non_string_path_segments() {
        let result = VAPOR_RESOLVER.extract(
            "UserController.swift",
            "router.get(\"users\", User.parameter, \"edit\", use: self.editUserHandler)\n",
        );
        assert_eq!(result.nodes[0].name, "GET /users/edit");
        assert_eq!(result.references[0].reference_name, "editUserHandler");
    }

    // it('ignores non-route .get calls that lack use: (e.g. Environment.get)')
    #[test]
    fn ignores_non_route_get_calls_that_lack_use() {
        let result = VAPOR_RESOLVER.extract(
            "configure.swift",
            "let host = Environment.get(\"DATABASE_HOST\") ?? \"localhost\"\n",
        );
        assert!(result.nodes.is_empty());
    }
}
