# 范围：模板标记解析器（Razor / Blazor / Thymeleaf）

状态：**P1+P2+@code 已实施**（提交 59b8de2 指令/标签、90c5f39 @code
代表团）在 `feat/cross-language-impact-coverage` 上。 Razor/Blazor 标记已解析
(`src/extraction/razor-extractor.ts`)。剩余：`@using` 命名空间消歧
对于 DTO-vs-entity 名称冲突（剩余的 ASP.NET 间隙）和 Thymeleaf/Django
（P4，延迟 - 弱代码链接）。作者于 2026 年 6 月 4 日。

## 问题

影响图是根据引擎解析的代码构建的。 **模板标记不是
已解析**，因此引用的任何代码隐藏、组件、视图模型或 DTO
*仅*从标记看来它没有依赖于回购协议。注重惯例
框架这是排除框架条目后的主要剩余差距：

| 框架 | 应用程序 | FAIR 覆盖范围（不包括条目） | 残留原因 |
|---|---|---|---|
| 网络平台 | 网上网上商店 | **77.2%** (115/149) | Razor `.cshtml` + Blazor `.razor` 参考 `.cs` 我们不解析 |
| 春天 | 宠物诊所 | 65.2% | 主要是 Spring Data 代理 + JPA，**不是**模板（Thymeleaf 链接很弱） |
| 姜戈 | Django-真实世界 | 74.1% | 信号/DRF/字符串配置，**不是**模板 |

**此功能主要是 ASP.NET (Razor + Blazor) 的胜利。** Thymeleaf 和 Django
模板仅弱链接到代码（模板→模板片段+模糊
模型属性字符串），而这些框架的真正差距在其他地方 - 所以它们
这里的优先级明显较低。

### 量化目标（eShopOnWeb，排除进入后的34个剩余零）

- **~20 个标记可通过此功能覆盖**：
  - 5 MVC `ViewModels/*` ← 剃刀 `@model X`
  - 7 `BlazorShared/Models/*` (DTO) ← Blazor `@bind` / 组件参数
  - 6 `BlazorAdmin/*` C# 组件 ← Blazor `<Component/>` 标签
  - 1 `BasketComponent` 视图组件 ← `<vc:basket>` / `Component.InvokeAsync`
  - 1 Razor 页面助手
- **~13 未涵盖**（单独的边界 - 反射/代理 + 值读取）：AutoMapper
`MappingProfile`、招摇 `CustomSchemaFilters`/`ImageValidators`、`ExceptionMiddleware`、
运行状况检查、`Constants`（静态成员读取）、`Buyer` 实体。

**诚实的上限：ASP.NET ~77% → ~90%**，而不是 95%。最后~10% 是反射/代理
（AutoMapper、Swagger、DI/中间件注册）+ C# static-const 读取 — a
*单独的*功能（反射建模+将静态成员传递扩展到C#）。

## 要提取的参考模式（优先）

| 普里 | 格式 | 标记构造 | 边缘发射 | 决心 |
|---|---|---|---|---|
| P1 | 剃须刀 `.cshtml`/`.razor` | `@model Foo` / `@inherits X<Foo>` | `references` | 型号/VM 类别 `Foo` |
| P1 | 剃刀/刀片 | `@inject IBar bar` | `references` | 服务类型 `IBar` |
| P2 | 开拓者`.razor` | `<MyComponent .../>`（帕斯卡命名法元素） | `references` | 组件类别（`.razor` 或 `.cs : ComponentBase`） |
| P2 | 开拓者`.razor` | `@typeof(MainLayout)`、`@inherits LayoutBase` | `references` | 类型 |
| P3 | 剃须刀`.cshtml` | `<partial name="_X"/>`、`<vc:basket>`、`Component.InvokeAsync("X")` | `references` | 局部视图 / `XViewComponent` |
| P3 | 剃须刀`.cshtml` | `asp-page="./Register"`、`asp-controller`/`asp-action` | `references` | 页面/控制器操作 |
| P4（推迟） | 百里香叶 `.html` | `th:replace="~{frag :: x}"` | `references` | 模板片段（模板→仅模板） |
| P4（推迟） | 姜戈 `.html` | `{% extends %}` / `{% include %}` / `{% url 'n' %}` | `references` | 模板/命名路线 |

`asp-for="Prop"`、`th:field="*{prop}"`（属性字符串绑定）是数据流
前沿——**超出范围**（需要模型类型推理；低值，高噪声）。

## 架构——遵循现有的独立提取器模式

该引擎已经具有非树木保护提取器（`svelte-extractor.ts`，
`vue-extractor.ts`、`liquid-extractor.ts`)：采用 `(filePath, source)` 的类，
返回 `{ nodes, references }`，在两个地方接线。准确镜像：

1. **`src/extraction/grammars.ts`** — 将扩展映射到合成语言：
`.cshtml`/`.razor` → `'razor'`，（稍后）`templates/` 下的 `.html` → `'thymeleaf'`。
（Django `.html` 与纯 HTML 不明确 — `templates/` 路径上的门或
`{% %}`/`{{ }}` 内容嗅探，就像框架解析器所做的那样。）
2. **`src/extraction/tree-sitter.ts`** — 通过扩展分派到新的
`RazorExtractor`（和 `ThymeleafExtractor`），与 `SvelteExtractor` 完全相同
已发送（~第 4025 行）。
3. **`src/extraction/razor-extractor.ts`**（新） - 正则表达式/行扫描（标记为
高度风格化；不需要语法，与 Liquid/Svelte 模板扫描相同）：
   - 为文件发出一个 `component` 节点（因此 `.razor` 组件可链接为
`<X/>` 目标并且该文件是图公民）。
   - 根据上面的 P1–P3 模式发出 `references`，`fromNodeId` = 文件/组件
节点，`referenceKind: 'references'`，`language: 'razor'`。
   - **代码隐藏链接：** a `Foo.razor` + `Foo.razor.cs`（部分类） - 发出
`references`（或依赖相同的基本名称），因此标记的参考文献也相信
代码隐藏。 （eShop 的 Blazor 组件是普通的 `.cs : ComponentBase`，名为
`<ToastComponent/>` → 按类名解析； `.razor.cs` 部分情况是
另一种形状。）

**分辨率：不需要新的解析器。**发出的引用是普通的 `references`
按名称到类/组件；现有的名称匹配器可以解析它们（`@model
RegisterModel` → class `RegisterModel`; `<ToastComponent/>` → class `ToastComponent`)。
应用已经到位的**相同的跨家庭语言门** - 必须有 `razor` 参考
解析为 `csharp` 符号，因此将 `razor` 添加到 `web`/dotnet 系列或处理
`razor`↔`csharp` 作为同族（否则来自提交 082353e 的门会丢弃它）。
**这是一个解析器端的更改**，必须完成，否则每个边缘都会被关闭。

## 节点/边形状和不变量

- 每个模板文件 +1 `component` 节点（真正的新符号 — 如 `.svelte`/`.vue`）。
节点数量仅随模板文件数量增长； **没有每个标签节点爆炸**
（组件标签变成 `references` 边，而不是节点）。
- 所有边均为`references`（按冲击力计算/`affected`/`getFileDependents`，
不是由 `callers`/`callees` — 匹配 `route`/`component` 边已经表现的方式）。
- 幂等重新索引；重新运行后节点数保持稳定。

## 定相

- **P1（最高价值/努力比）：** Razor `@model` + `@inject` for `.cshtml` AND
`.razor`。涵盖 5 个 ViewModel + 注入服务。 + 解析器家族门修复。
- **P2：** Blazor `<PascalComponent/>` 标签 + `@typeof`/`@inherits` + 代码隐藏链接。
涵盖 6 个 Blazor `.cs` 组件 + 7 个 DTO（通过组件 params/`@bind`）。
- **P3：** 剃须刀 `<partial>` / `<vc:>` / `Component.InvokeAsync` / `asp-page`。
- **P4（推迟/可能跳过）：** Thymeleaf + Django 模板 - 弱代码链接，
低覆盖回报；仅当 Thymeleaf/Django 应用程序优先时才重新访问。

## 边缘案例和风险

- **PascalCase 标签与 HTML 元素：** 只有 `[A-Z]`-初始标签是 Blazor 组件
（HTML 为小写）— 安全鉴别器。跳过已知的框架组件
（`<Router>`、`<Found>`、`<LayoutView>`、`<RouteView>`、`<CascadingValue>`）通过
内置集，或者只是让它们无法解析（没有错误边缘 - 它们不在回购中）。
- **`_Imports.razor` `@using`:** 命名空间导入，而不是代码引用 — 忽略（或发出
`imports` 到命名空间，低值）。
- **通用组件 `<Grid TItem="CatalogItem">`:** 将 type-arg 捕获为
`references` 到 `CatalogItem`（额外的 DTO 覆盖范围）。
- **名称冲突：**组件/模型名称通常是唯一的；依靠
名称匹配器现有的邻近度评分。另一种语言的同名类是
被家族大门挡住。
- **Razor `@{ ... }` C# 块：** 包含真正的 C#（调用，`new`） — P-future；正则表达式
扫描 C# 内部标记很嘈杂。推迟（上面的指令是胜利）。
- **`.razor` 不是 `.cs`：** 必须添加到 `grammars.ts` + 索引器的包含 glob
（验证 `.razor`/`.cshtml` 不在默认排除中）。

## 验证（根据引擎的方法）

1. 构建`RazorExtractor`； `__tests__/extraction.test.ts` 中的单元测试（`.cshtml`
其中 `@model X` 覆盖 `X`； `.razor` 和 `<ToastComponent/>` 覆盖它；一个 HTML
`<div>` 不会创建边缘）。
2. 重新测量之前/之后的 eShopOnWeb FAIR 覆盖率 (`/tmp/faircov.cjs`)：目标
77%→~90%； **节点数稳定**（仅+模板文件组件节点）；残差
零仅是反射/值读取集。
3. 非 .NET 控件 (gin/requests) 和无 Razor C# 上没有回归
存储库（cs-mediatr/cs-polly 不变）。
4. 在此文档中记录 + 覆盖范围交接。

## 努力

- P1：~0.5 天（提取器骨架 + `@model`/`@inject` 扫描 + 家庭门修复 + 测试）。
- P2：~1 天（Blazor 标签 + 代码隐藏 + 通用类型参数）。
- P3：~0.5 天。 P4 (Thymeleaf/Django)：约 1-2 天，投资回报率低 — 推迟。
- **ASP.NET 获胜总计 (P1+P2+P3)：~2 天 → ASP.NET ~90%。**

## 非目标（以及 95% 的会议应用程序仍然需要的内容）

此功能不会关闭：反射/代理注册（Spring数据存储库
代理、AutoMapper 配置文件、Swagger 过滤器、DI 容器/中间件）、属性 -
字符串数据绑定 (`asp-for`/`th:field`)，或 C# 静态常量值读取
(`Constants.X`)。达到字面上 95% 的常规应用程序还需要 **反思/
DI 注册建模** 传递和**将静态成员传递扩展到 C#/TS** —
分别跟踪。标记解析是最大、最独立的步骤。
