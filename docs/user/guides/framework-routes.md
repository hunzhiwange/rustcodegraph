# 框架路由

RustCodeGraph 会把 URL 模式关联到处理这些请求的处理器。

RustCodeGraph 会检测 Web 框架的路由文件，并生成 `route` 节点，再通过 `references` 边把它们连接到对应的处理器类或函数。查询某个视图或控制器的调用方时，就能看到绑定到它的 URL 模式。

| Framework | 可识别的形式 |
|---|---|
| **Django** | `urls.py` 中的 `path()`、`re_path()`、`url()`、`include()`（CBV `.as_view()`、点分路径） |
| **Flask** | `@app.route('/path', methods=[…])`、蓝图路由 |
| **FastAPI** | `@app.get(…)`、`@router.post(…)`、所有标准方法 |
| **Express** | 带中间件链的 `app.get(…)`、`router.post(…)` |
| **NestJS** | `@Controller` + `@Get/@Post/…`、GraphQL 解析器、消息/事件模式、WebSocket 订阅 |
| **Laravel** | `Route::get()`、`Route::resource()`、`Controller@action`、元组语法 |
| **Drupal** | `*.routing.yml` 路由；`.module`/`.theme`/`.install`/`.inc` 中的 `hook_*` 实现 |
| **Rails** | `get '/x', to: 'users#index'`、hash-rocket 语法 |
| **Spring** | 方法上的 `@GetMapping`、`@PostMapping`、`@RequestMapping` |
| **Gin / chi / gorilla / mux** | `r.GET(…)`、`router.HandleFunc(…)` |
| **Axum / actix / Rocket** | `.route("/x", get(handler))` |
| **ASP.NET** | action 方法上的 `[HttpGet("/x")]` 属性 |
| **Vapor** | `app.get("x", use: handler)` |
| **React Router** / **SvelteKit** | 路由组件节点 |

路由解析会自动完成，无需配置。如果框架文件被识别，它的路由会在下一次索引或同步后出现在图中。
