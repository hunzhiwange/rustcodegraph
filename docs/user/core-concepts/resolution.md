# 决议与框架

RustCodeGraph 如何连接引用并将路由链接到处理程序。

提取产生节点和原始边缘； **解析**将名字变成真正的联系。

## 参考分辨率

解析后，RustCodeGraph解析为：

- **导入** → 它们指向的源文件（包括 tsconfig 路径别名和 Cargo 工作区成员）。 
- **调用**→它们的定义，通过导入解析和名称匹配。 
- **继承** → 类型之间的 `extends` / `implements`。

## 框架意识

RustCodeGraph 识别 Web 框架路由文件并发出由 `references` 边链接到其处理程序类或函数的 `route` 节点 - 因此查询视图或控制器的调用者会显示绑定它的 URL 模式。 请参阅 [Framework Routes](../guides/framework-routes.md) 了解公认框架的完整列表。

## 动态调度覆盖范围

静态解析会错过计算调用和间接调用，因此流程可能会在动态调度时中断。 RustCodeGraph 使用合成器桥接了其中几个边界，因此流程端到端连接：

- 回调/观察者注册
-  `EventEmitter`频道
-  React 重新渲染 (`setState` → `render`)
-  JSX 子组件（`render` → 子组件）
-  Django ORM 描述符

每个合成边都标记为 `provenance: 'heuristic'` 以及连接它的站点，并且在路径穿过它的任何地方都内联显示。
