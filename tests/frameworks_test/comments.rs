use super::*;

mod framework_extractors_ignore_commented_out_routes {
    use super::*;

    // describe('framework extractors ignore commented-out routes')
    // it('django: skips line-comment and docstring routes')
    #[test]
    fn django_skips_line_comment_and_docstring_routes() {
        let src = r#"
# urls.py example:
# path('/admin/', AdminPanel.as_view())
"""
Other routing example:
    path('/users/', UserListView.as_view())
"""
urlpatterns = [path('/real/', RealView.as_view())]
"#;
        let result = DJANGO_RESOLVER.extract("app/urls.py", src);
        assert_eq!(names(&result.nodes), vec!["/real/"]);
    }

    // it('flask: skips commented-out @app.route')
    #[test]
    fn flask_skips_commented_out_app_route() {
        let src = r#"
# @app.route('/fake')
# def fake_view():
#     return ''

@app.route('/real')
def real_view():
    return ''
"#;
        let result = FLASK_RESOLVER.extract("app.py", src);
        assert_eq!(names(&result.nodes), vec!["GET /real"]);
        assert_eq!(reference_names(&result.references), vec!["real_view"]);
    }

    // it('fastapi: skips docstring example routes')
    #[test]
    fn fastapi_skips_docstring_example_routes() {
        let src = r#"
"""
Example:
    @app.get('/in-docstring')
    async def doc():
        pass
"""
@app.get('/real')
async def real_handler():
    return {}
"#;
        let result = FASTAPI_RESOLVER.extract("main.py", src);
        assert_eq!(names(&result.nodes), vec!["GET /real"]);
        assert_eq!(reference_names(&result.references), vec!["real_handler"]);
    }

    // it('express: skips // and /* */ commented routes')
    #[test]
    fn express_skips_slash_and_block_commented_routes() {
        let src = r#"
// app.get('/fake', fakeHandler);
/* router.post('/also-fake', otherHandler); */
app.get('/real', realHandler);
"#;
        let result = EXPRESS_RESOLVER.extract("routes.ts", src);
        assert_eq!(names(&result.nodes), vec!["GET /real"]);
        assert_eq!(reference_names(&result.references), vec!["realHandler"]);
    }

    // it('laravel: skips // # and /* */ commented Route::* calls')
    #[test]
    fn laravel_skips_line_hash_and_block_commented_route_calls() {
        let src = r#"<?php
// Route::get('/fake', [FakeController::class, 'index']);
# Route::get('/also-fake', 'FakeController@show');
/* Route::post('/another-fake', [X::class, 'y']); */
Route::get('/real', [RealController::class, 'index']);
"#;
        let result = LARAVEL_RESOLVER.extract("routes/web.php", src);
        assert_eq!(names(&result.nodes), vec!["GET /real"]);
        assert_eq!(
            reference_names(&result.references),
            vec!["RealController@index"]
        );
    }

    // it('rails: skips =begin/=end and # commented routes')
    #[test]
    fn rails_skips_begin_end_and_hash_commented_routes() {
        let src = r#"
# get '/fake', to: 'fake#index'
=begin
get '/also-fake', to: 'fake#show'
=end
get '/real', to: 'real#index'
"#;
        let result = RAILS_RESOLVER.extract("config/routes.rb", src);
        assert_eq!(names(&result.nodes), vec!["GET /real"]);
        assert_eq!(reference_names(&result.references), vec!["real#index"]);
    }

    // it('spring: skips // and /* */ commented @GetMapping')
    #[test]
    fn spring_skips_slash_and_block_commented_get_mapping() {
        let src = r#"
// @GetMapping("/fake")
// public List<X> fake() { return null; }

/* @PostMapping("/also-fake")
   public void alsoFake() {} */

@GetMapping("/real")
public List<User> listUsers() { return users; }
"#;
        let result = SPRING_RESOLVER.extract("UserController.java", src);
        assert_eq!(names(&result.nodes), vec!["GET /real"]);
        assert_eq!(reference_names(&result.references), vec!["listUsers"]);
    }

    // it('go: skips // and /* */ commented router.METHOD calls')
    #[test]
    fn go_skips_slash_and_block_commented_router_method_calls() {
        let src = r#"
// r.GET("/fake", fakeHandler)
/* r.POST("/also-fake", anotherHandler) */
r.GET("/real", listUsers)
"#;
        let result = GO_RESOLVER.extract("main.go", src);
        assert_eq!(names(&result.nodes), vec!["GET /real"]);
        assert_eq!(reference_names(&result.references), vec!["listUsers"]);
    }

    // it('rust: skips // and nested /* */ commented .route() calls')
    #[test]
    fn rust_skips_slash_and_nested_block_commented_route_calls() {
        let src = r#"
// .route("/fake", get(fake_handler))
/* outer /* inner .route("/inner-fake", get(x)) */ still .route("/outer-fake", get(y)) */
let app = Router::new().route("/real", get(list_users));
"#;
        let result = RUST_RESOLVER.extract("main.rs", src);
        assert_eq!(names(&result.nodes), vec!["GET /real"]);
        assert_eq!(reference_names(&result.references), vec!["list_users"]);
    }

    // it('aspnet: skips // and /* */ commented [HttpGet] attributes')
    #[test]
    fn aspnet_skips_slash_and_block_commented_http_get_attributes() {
        let src = r#"
// [HttpGet("/fake")]
// public IActionResult Fake() { return Ok(); }

/* [HttpPost("/also-fake")]
   public IActionResult AlsoFake() { return Ok(); } */

[HttpGet("/real")]
public IActionResult ListUsers() { return Ok(); }
"#;
        let result = ASPNET_RESOLVER.extract("UserController.cs", src);
        assert_eq!(names(&result.nodes), vec!["GET /real"]);
        assert_eq!(reference_names(&result.references), vec!["ListUsers"]);
    }

    // it('vapor: skips // and /* */ commented app.METHOD calls')
    #[test]
    fn vapor_skips_slash_and_block_commented_app_method_calls() {
        let src = r#"
// app.get("fake", use: fakeHandler)
/* app.post("also-fake", use: anotherHandler) */
app.get("real", use: listUsers)
"#;
        let result = VAPOR_RESOLVER.extract("routes.swift", src);
        assert_eq!(names(&result.nodes), vec!["GET /real"]);
        assert_eq!(reference_names(&result.references), vec!["listUsers"]);
    }

    // it('nestjs: skips // and /* */ commented decorators')
    #[test]
    fn nestjs_skips_slash_and_block_commented_decorators() {
        let src = r#"
@Controller('users')
export class UsersController {
  // @Get('fake')
  // fake() {}
  /* @Post('also-fake')
     alsoFake() {} */
  @Get('real')
  real() {}
}
"#;
        let result = NESTJS_RESOLVER.extract("users.controller.ts", src);
        assert_eq!(names(&result.nodes), vec!["GET /users/real"]);
        assert_eq!(reference_names(&result.references), vec!["real"]);
    }
}
