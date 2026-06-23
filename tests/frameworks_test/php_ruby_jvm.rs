use super::*;

mod laravel_resolver_extract {
    use super::*;

    // describe('laravelResolver.extract')
    // it('extracts route with controller tuple syntax')
    #[test]
    fn extracts_route_with_controller_tuple_syntax() {
        let result = LARAVEL_RESOLVER.extract(
            "routes/web.php",
            "Route::get('/users', [UserController::class, 'index']);\n",
        );
        assert_eq!(result.nodes[0].name, "GET /users");
        assert_eq!(result.references[0].reference_name, "UserController@index");
    }

    // it('extracts route with Controller@action syntax')
    #[test]
    fn extracts_route_with_controller_action_syntax() {
        let result = LARAVEL_RESOLVER.extract(
            "routes/web.php",
            "Route::post('/users', 'UserController@store');\n",
        );
        assert_eq!(result.references[0].reference_name, "UserController@store");
    }

    // it('extracts resource route')
    #[test]
    fn extracts_resource_route() {
        let result = LARAVEL_RESOLVER.extract(
            "routes/web.php",
            "Route::resource('users', UserController::class);\n",
        );
        assert_eq!(result.nodes[0].kind, NodeKind::Route);
        assert_eq!(result.references[0].reference_name, "UserController");
    }
}

mod rails_resolver_extract {
    use super::*;

    // describe('railsResolver.extract')
    // it('extracts route with controller#action syntax')
    #[test]
    fn extracts_route_with_controller_action_syntax() {
        let result =
            RAILS_RESOLVER.extract("config/routes.rb", "get '/users', to: 'users#index'\n");
        assert_eq!(result.nodes[0].name, "GET /users");
        assert_eq!(result.references[0].reference_name, "users#index");
    }

    // it('extracts route without to: keyword')
    #[test]
    fn extracts_route_without_to_keyword() {
        let result =
            RAILS_RESOLVER.extract("config/routes.rb", "post '/items' => 'items#create'\n");
        assert_eq!(result.references[0].reference_name, "items#create");
    }
}

mod spring_resolver_extract {
    use super::*;

    // describe('springResolver.extract')
    // it('extracts route with @GetMapping and next method')
    #[test]
    fn extracts_route_with_get_mapping_and_next_method() {
        let src = r#"
@GetMapping("/users")
public List<User> listUsers() {
  return users;
}
"#;
        let result = SPRING_RESOLVER.extract("UserController.java", src);
        assert_eq!(result.nodes[0].name, "GET /users");
        assert_eq!(result.references[0].reference_name, "listUsers");
    }

    // it('extracts a Kotlin @GetMapping with a fun handler')
    #[test]
    fn extracts_a_kotlin_get_mapping_with_a_fun_handler() {
        let src = r#"
@GetMapping("/vets")
fun showVetList(model: MutableMap<String, Any>): String {
  return "vets"
}
"#;
        let result = SPRING_RESOLVER.extract("VetController.kt", src);
        assert_eq!(result.nodes[0].name, "GET /vets");
        assert_eq!(result.references[0].reference_name, "showVetList");
        assert_eq!(result.nodes[0].language, Language::Kotlin);
    }

    // it('joins a Kotlin class @RequestMapping prefix and skips a stacked annotation')
    #[test]
    fn joins_a_kotlin_class_request_mapping_prefix_and_skips_a_stacked_annotation() {
        let src = r#"
@RestController
@RequestMapping("/owners")
class OwnerController {
  @GetMapping("/{ownerId}")
  @ResponseBody
  fun showOwner(@PathVariable ownerId: Int): String {
    return "owner"
  }
}
"#;
        let result = SPRING_RESOLVER.extract("OwnerController.kt", src);
        assert_eq!(result.nodes[0].name, "GET /owners/{ownerId}");
        assert_eq!(result.references[0].reference_name, "showOwner");
    }
}

mod play_resolver_extract_conf_routes {
    use super::*;

    // describe('playResolver.extract (conf/routes)')
    // it('extracts METHOD /path Controller.action routes, dropping the package + args')
    #[test]
    fn extracts_method_path_controller_action_routes_dropping_package_and_args() {
        let src = r#"# Routes
GET     /                    controllers.Application.index
GET     /computers           controllers.Application.list(p: Int ?= 0, s: Int ?= 2)
POST    /computers           controllers.Application.save
-> /v1/posts                 v1.post.PostRouter
"#;
        let result = PLAY_RESOLVER.extract("conf/routes", src);
        assert_eq!(
            names(&result.nodes),
            vec!["GET /", "GET /computers", "POST /computers"]
        );
        assert_eq!(
            reference_names(&result.references),
            vec!["Application.index", "Application.list", "Application.save"]
        );
    }

    // it('only runs on Play routes files')
    #[test]
    fn only_runs_on_play_routes_files() {
        let result = PLAY_RESOLVER.extract("app/Foo.scala", "GET / controllers.X.y");
        assert!(result.nodes.is_empty());
    }
}

mod play_routes_file_detection {
    use super::*;

    // describe('Play routes file detection')
    // it('recognizes conf/routes (extensionless) and *.routes as source files')
    #[test]
    fn recognizes_conf_routes_extensionless_and_routes_as_source_files() {
        assert!(is_play_routes_file("conf/routes"));
        assert!(is_play_routes_file("myapp/conf/routes"));
        assert!(is_play_routes_file("conf/admin.routes"));
        assert!(is_source_file("conf/routes"));
        assert!(!is_play_routes_file("src/routes.ts"));
    }
}
