use super::*;

mod nestjs_resolver_extract_http {
    use super::*;

    // describe('nestjsResolver.extract - HTTP')
    // it('joins @Controller prefix with @Get and links the handler')
    #[test]
    fn joins_controller_prefix_with_get_and_links_the_handler() {
        let src = r#"
@Controller('users')
export class UsersController {
  @Get()
  findAll() { return []; }
}
"#;
        let result = NESTJS_RESOLVER.extract("users.controller.ts", src);
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].kind, NodeKind::Route);
        assert_eq!(result.nodes[0].name, "GET /users");
        assert_eq!(result.references[0].reference_name, "findAll");
        assert_eq!(
            result.references[0].reference_kind,
            ReferenceKind::References
        );
        assert_eq!(result.references[0].from_node_id, result.nodes[0].id);
    }

    // it('joins controller prefix with a method-level path param')
    #[test]
    fn joins_controller_prefix_with_a_method_level_path_param() {
        let src = r#"
@Controller('cats')
export class CatsController {
  @Get(':id')
  findOne(@Param('id') id: string) { return id; }
}
"#;
        let result = NESTJS_RESOLVER.extract("cats.controller.ts", src);
        assert_eq!(result.nodes[0].name, "GET /cats/:id");
        assert_eq!(result.references[0].reference_name, "findOne");
    }

    // it('handles an empty @Controller() and empty @Post()')
    #[test]
    fn handles_an_empty_controller_and_empty_post() {
        let src = r#"
@Controller()
export class AppController {
  @Post()
  create() {}
}
"#;
        let result = NESTJS_RESOLVER.extract("app.controller.ts", src);
        assert_eq!(result.nodes[0].name, "POST /");
        assert_eq!(result.references[0].reference_name, "create");
    }

    // it('covers HTTP verbs and skips intervening method decorators')
    #[test]
    fn covers_http_verbs_and_skips_intervening_method_decorators() {
        let src = r#"
@Controller('todos')
export class TodosController {
  @Put(':id')
  @UseGuards(AuthGuard)
  update(@Param('id') id: string) {}

  @Delete(':id')
  async remove(@Param('id') id: string) {}
}
"#;
        let result = NESTJS_RESOLVER.extract("todos.controller.ts", src);
        assert_eq!(
            names(&result.nodes),
            vec!["PUT /todos/:id", "DELETE /todos/:id"]
        );
        assert_eq!(
            reference_names(&result.references),
            vec!["update", "remove"]
        );
    }

    // it('attributes methods to the right controller when a file has two')
    #[test]
    fn attributes_methods_to_the_right_controller_when_a_file_has_two() {
        let src = r#"
@Controller('a')
export class AController {
  @Get('x')
  ax() {}
}

@Controller('b')
export class BController {
  @Get('y')
  by() {}
}
"#;
        let result = NESTJS_RESOLVER.extract("multi.controller.ts", src);
        assert_eq!(names(&result.nodes), vec!["GET /a/x", "GET /b/y"]);
    }
}

mod nestjs_resolver_extract_graphql {
    use super::*;

    // describe('nestjsResolver.extract - GraphQL')
    // it('emits QUERY/MUTATION nodes from a resolver, defaulting to the method name')
    #[test]
    fn emits_query_mutation_nodes_from_a_resolver_defaulting_to_the_method_name() {
        let src = r#"
@Resolver(() => User)
export class UsersResolver {
  @Query(() => [User])
  users() { return []; }

  @Mutation(() => User)
  createUser(@Args('input') input: CreateUserInput) {}
}
"#;
        let result = NESTJS_RESOLVER.extract("users.resolver.ts", src);
        assert_eq!(
            names(&result.nodes),
            vec!["QUERY users", "MUTATION createUser"]
        );
        assert_eq!(
            reference_names(&result.references),
            vec!["users", "createUser"]
        );
    }

    // it('uses an explicit operation name when given')
    #[test]
    fn uses_an_explicit_operation_name_when_given() {
        let src = r#"
@Resolver()
export class CatsResolver {
  @Query(() => Cat, { name: 'cat' })
  getCat() {}
}
"#;
        let result = NESTJS_RESOLVER.extract("cats.resolver.ts", src);
        assert_eq!(result.nodes[0].name, "QUERY cat");
    }

    // it('does NOT treat the REST @Query() parameter decorator as a GraphQL op')
    #[test]
    fn does_not_treat_the_rest_query_parameter_decorator_as_a_graphql_op() {
        let src = r#"
@Controller('search')
export class SearchController {
  @Get()
  search(@Query() query: SearchDto) { return query; }
}
"#;
        let result = NESTJS_RESOLVER.extract("search.controller.ts", src);
        assert_eq!(names(&result.nodes), vec!["GET /search"]);
    }
}

mod nestjs_resolver_extract_microservices_websockets {
    use super::*;

    // describe('nestjsResolver.extract - microservices & websockets')
    // it('extracts @MessagePattern and @EventPattern handlers')
    #[test]
    fn extracts_message_pattern_and_event_pattern_handlers() {
        let src = r#"
@Controller()
export class MathController {
  @MessagePattern({ cmd: 'sum' })
  accumulate(data: number[]) {}

  @EventPattern('user.created')
  handleUserCreated(data: any) {}
}
"#;
        let result = NESTJS_RESOLVER.extract("math.controller.ts", src);
        assert_eq!(
            names(&result.nodes),
            vec!["MESSAGE sum", "EVENT user.created"]
        );
        assert_eq!(
            reference_names(&result.references),
            vec!["accumulate", "handleUserCreated"]
        );
    }

    // it('extracts @SubscribeMessage handlers with the gateway namespace')
    #[test]
    fn extracts_subscribe_message_handlers_with_the_gateway_namespace() {
        let src = r#"
@WebSocketGateway({ namespace: 'chat' })
export class ChatGateway {
  @SubscribeMessage('message')
  handleMessage(@MessageBody() data: string) {}
}
"#;
        let result = NESTJS_RESOLVER.extract("chat.gateway.ts", src);
        assert_eq!(result.nodes[0].name, "WS chat:message");
        assert_eq!(result.references[0].reference_name, "handleMessage");
    }

    // it('extracts @SubscribeMessage without a namespace')
    #[test]
    fn extracts_subscribe_message_without_a_namespace() {
        let src = r#"
@WebSocketGateway()
export class EventsGateway {
  @SubscribeMessage('events')
  onEvent() {}
}
"#;
        let result = NESTJS_RESOLVER.extract("events.gateway.ts", src);
        assert_eq!(result.nodes[0].name, "WS events");
    }

    // it('returns empty for a non-JS/TS file')
    #[test]
    fn returns_empty_for_a_non_js_ts_file() {
        let result = NESTJS_RESOLVER.extract("thing.py", "@Controller(\"x\")");
        assert!(result.nodes.is_empty());
        assert!(result.references.is_empty());
    }
}

mod nestjs_resolver_detect {
    use super::*;

    // describe('nestjsResolver.detect')
    // it('detects @nestjs/* in package.json')
    #[test]
    fn detects_nestjs_in_package_json() {
        let mut context = MockResolutionContext::new().with_file_contents(&[(
            "package.json",
            r#"{"dependencies":{"@nestjs/common":"^10.0.0"}}"#,
        )]);
        assert!(NESTJS_RESOLVER.detect(&mut context));
    }

    // it('detects @Controller in a *.controller.ts file when package.json is absent')
    #[test]
    fn detects_controller_in_controller_ts_file_when_package_json_is_absent() {
        let mut context = MockResolutionContext::new()
            .with_file_contents(&[(
                "src/users.controller.ts",
                "@Controller('users')\nexport class UsersController {}",
            )])
            .with_all_files(&["src/users.controller.ts"]);
        assert!(NESTJS_RESOLVER.detect(&mut context));
    }

    // it('returns false for a non-Nest project')
    #[test]
    fn returns_false_for_a_non_nest_project() {
        let mut context = MockResolutionContext::new()
            .with_file_contents(&[("package.json", r#"{"dependencies":{"express":"^4"}}"#)]);
        assert!(!NESTJS_RESOLVER.detect(&mut context));
    }
}

mod nestjs_resolver_resolve {
    use super::*;

    // describe('nestjsResolver.resolve')
    // it('resolves an injected *Service reference to the class in a *.service.ts file')
    #[test]
    fn resolves_an_injected_service_reference_to_the_class_in_a_service_ts_file() {
        let svc_node = node(
            "class:src/users/users.service.ts:UsersService:3",
            NodeKind::Class,
            "UsersService",
            "src/users/users.service.ts::UsersService",
            "src/users/users.service.ts",
            Language::TypeScript,
            3,
            3,
        );
        let mut context = MockResolutionContext::with_nodes(vec![svc_node.clone()]);
        let reference = unresolved_ref(
            "class:src/users/users.controller.ts:UsersController:5",
            "UsersService",
            ReferenceKind::References,
            "src/users/users.controller.ts",
            Language::TypeScript,
        );
        let result = NESTJS_RESOLVER.resolve(&reference, &mut context);
        let result = result.expect("UsersService should resolve");
        assert_eq!(result.target_node_id, svc_node.id);
        assert_eq!(result.resolved_by, ResolvedBy::Framework);
        assert!(result.confidence >= 0.85);
    }

    // it('returns null for a name without a provider suffix')
    #[test]
    fn returns_null_for_a_name_without_a_provider_suffix() {
        let mut context = MockResolutionContext::new();
        let reference = unresolved_ref(
            "x",
            "doThing",
            ReferenceKind::References,
            "a.ts",
            Language::TypeScript,
        );
        assert!(NESTJS_RESOLVER.resolve(&reference, &mut context).is_none());
    }
}
