# 动态调度覆盖手册

**观众：** 克劳德特工正在继续这项工作。
**使命：**系统地弥补**动态的静态提取覆盖漏洞
跨**每种语言和框架 rustcodegraph 支持**进行调度**，并验证
每个都以相同的方式进行，因此交叉符号*流*存在于图中的任何地方。

> 这是顶级剧本。一种机制的深层设计（回调
> 合成器）位于 [`callback-edge-synthesis.md`](./callback-edge-synthesis.md) 中。
> 完整的调查背景+结果：自动记忆`project_rustcodegraph_read_displacement`。

> **更新 (2026-06-01)：** `trace` 和 `context` MCP 工具已
> **已删除** — `rustcodegraph_explore` 现在是单一曲面工具。它的“流程”部分
> (`format_flow_section`) 显示此剧本所涉及的合成边，并且
> 您可以使用 `rustcodegraph_explore` / `rustcodegraph agent-eval probe-explore` 验证覆盖范围。
> 其中下面的文字写为 `trace(a, b)` 或在工具中列出 `trace`/`context`，
> 将其解读为“a→b 流程，现已浮出水面并通过探索进行验证。”合成器和
> 覆盖矩阵不变。

---

## 1. 目标（为什么这很重要）

rustcodegraph 的价值在于 **地图** — 回答结构/流程问题
（`trace`、`impact`、调用者，“X 如何到达 Y”）grep/Read 不能。代理商
**仅当足够时**才会使用 rustcodegraph 而不是 Read。我们证明了
根据经验（见记忆），充足性的杠杆是**覆盖率**，而不是
提示/挂钩/新工具：当图表中缺少流时，代理会读取
用于重建它的文件；当流量*在*图中时，代理可以回答
完全没有阅读。

**在 exalidraw 上进行端到端验证：** 关闭更新流程漏洞后，2/3
无头代理运行回答了“更新如何到达屏幕”的问题
**读取 0 和完整答案** — 以前不可能，因为关键边缘不在
图表。 （警告：覆盖*启用*无读取路径；代理通过读取确认
方差意味着它不会“强制”它。完整性无条件提高。）

我们的使命是让**所有**语言/框架都实现这一点。

---

## 2.问题类别：动态调度

静态树守护者提取捕获显式调用（`foo()`、`this.bar()`）。它
**错过**任何其目标是计算/间接的调用。四种重复出现的形状，带有
**难度梯度**（先做便宜的）：

| # | 形状 | 例子 | 修复机制 | 成本 |
|---|---|---|---|---|
| 1 | **命名属性/描述符** | Django `self._iterable_class(self)` | 框架解析器（`claims_reference` + `resolve()`） | **便宜的** |
| 2 | **现场支持观察员** | `onUpdate(cb)` + `for(cb of cbs)cb()` | 回调合成器（全图传递） | 中等的 |
| 3 | **字符串键控事件发射器** | `on('e',fn)` / `emit('e')` | 回调合成器（事件键控） | 中等的 |
| 4 | **内联回调处理程序** | `on('e', function h(){})` / `() => {}` | 提取（命名）+合成器链接通过主体（匿名） | 名称：廉价 · 匿名：困难 |
| 5 | **关闭-收集派送** | 斯威夫特 `validators.write{$0.append(v)}` … `validators.forEach{$0()}` | 回调合成器（`closure_collection_edges`，元素调用门控） | 中等的 |

推动机制选择的关键区别：
- **存在要解析的命名引用**（`_iterable_class` 是属性名称）→ **解析器**。
- **不存在引用**（`cb()` 是匿名的；需要注册器↔调度器关联）→ **合成器**。

---

## 3. 工作示例（两种机制，端到端）

### 3a. Django ORM 描述符 — **解析器** 模式 (Python)
- **漏洞：** `QuerySet._fetch_all` 调用 `self._iterable_class(self)` （运行时选择的
iterable，默认`ModelIterable`），其`__iter__`运行SQL编译器。静止的
解析无法解析属性为可调用 → `_fetch_all` 的唯一被调用者是
`_prefetch_related_objects`; `trace(_fetch_all, execute_sql)` 没有返回路径。
- **修复：** Django 解析器通过以下方式声明未解析的 `_iterable_class` 引用
name-exists 预过滤器，然后将其解析为 `ModelIterable.__iter__`。
- **文件：** `src/resolution/types.rs`（`FrameworkResolver` 上的 `claims_reference?`），
`src/resolution/index.rs`（前置过滤器参考`claims_reference`），
`src/resolution/frameworks/python.rs`（Django 解析器 + `claims_reference` +
`resolveModelIterableIter`）。
- **结果：** `trace(_fetch_all, execute_sql)` → `_fetch_all → __iter__ → execute_sql`（3 跳）。

### 3b. Excalidraw 观察者 + EventEmitter — **合成器** (TS)
- **孔：** `Scene.triggerUpdate` 做 `for (cb of this.callbacks) cb()`； `triggerRender`
通过 `scene.onUpdate(this.triggerRender)` 注册。 `triggerUpdate →
triggerRender` edge is dynamic → `trace`未返回路径；整个更新流程中断了。
- **修复：** 检测注册商/调度员通道、关联的全图传递
注册位点，并合成 `dispatcher → callback` 边。再加上提取
**命名**内联回调，因此像express的`function onmount(){}`这样的处理程序是节点。
- **文件：** `src/resolution/callback_synthesizer.rs`（通行证 — 现场观察员 +
EventEmitter), `src/resolution/index.rs` (之后调用 `synthesize_callback_edges()`
基本分辨率），`src/extraction/tree_sitter.rs`（`visit_function_body`
提取命名的嵌套函数）。
- **结果：** `trace(mutateElement, triggerRender)` → 3 跳；快递`use → onmount`。

### 3c. Alamofire 延迟验证 — 闭包集合调度 (Swift)
- **孔：** `DataRequest.validate(_:)` 构建闭合和 `validators.write { $0.append(validator) }`；
基础 `Request.didCompleteTask` 通过 `validators.forEach { $0() }` 运行它们。追加并
分派生活在*不同的文件和类*（子类附加，基础迭代）和
字段是 Swift `Protected<[@Sendable () -> Void]>` - 所以既不是相同文件配对，也不是
基于名称的注册商匹配 (`onX`/`subscribe`/...) 达到此目的。 `trace(didCompleteTask, validate)`
没有返回路径；代理 grep `validators` 并读取三个文件来重建它。
- **修复：** `closure_collection_edges` (`callback_synthesizer.rs`)。 **调度程序**迭代集合
*调用每个元素* (`coll.forEach { $0() }` / `{ it() }`); **注册商** 附加一个闭包
同名字段（`.append`/`.add`/`.push`/`.insert`，包括 Swift `.write { $0.append }`）。这
element-invoke (`$0(` / `it(`) 是精确的 **gate** — 它证明集合持有闭包 —
因此，无论有多少 `.append`，没有闭包集合调度的存储库都会产生 **0 边**
它拥有的网站。按字段名称全局配对调度程序 → 注册程序（需要跨文件/类），
扇出上限。以两种方式出现：在 `trace` 中内联，以及作为“动态调度链接”
`rustcodegraph_explore` (`format_dynamic_dispatch_links`) 中的符号”部分，因此关系显示均匀
当代理仅命名 `validate`，而不是耗尽列表的 `didCompleteTask` 时。
- **文件：** `src/resolution/callback_synthesizer.rs` (`closure_collection_edges`),
`src/mcp/tools.rs`（`format_dynamic_dispatch_links` + 探索合成器链接部分）。
- **结果：** `trace(didCompleteTask, validate)` 连接到闭包集合跃点 +
`validators.write { $0.append }` 接线现场内联。 Alamofire 上的 9 个精确边缘
(`validators`/`streams`/`finishHandlers`/`requestsToRetry`)，**每个非 Swift 控件均为 0**。
强制仅使用 rustcodegraph（Read+Grep+Bash 被阻止）：3/3 正确运行答案构建/发送/验证。

### 3d.洞察力——“采用层”可以隐藏跟踪端点错误（Alamofire）
Alamofire（110 个文件）是自述文件中最弱的存储库，被注销为“小型存储库层”
（本机 grep 很便宜，因此代理无论如何都会读取）。事实并非如此。阅读**文字记录**——每
`Read` 的 `file_path`+ 偏移量和其前面的辅助文本 — 浮现了特工自己的话：
*“轨迹与同名符号发生碰撞（44 `request`、8 `task`），让我逐行读取。”*
`trace` 的端点消歧（`scorePair`，仅共享目录前缀）正在解决
**空委托/协议存根**的重载名称 - `request` → `EventMonitor.request(){}`
（1 行无操作）在真实的 `Session.request` 上，因为两个不相关的 `Source/Features/` 存根
共享比正确的 `Source/Core/` 对更深的目录前缀。垃圾追踪→人工读取，
有时是一个螺旋（一次运行 12 次读取/11 次 grep）。 **修复：**端点相关性评分
惩罚空存根（≤1 正文行）和测试文件符号；在真正的方法中，它是
平坦，因此路径邻近度 (cosmos `EndBlocker`) 不受影响。结果 (n=8)：WITH-arm 工具调用
12 → 8 中位数，读数**方差崩溃**（0–12 → 1–4 — 崩溃*是*
痕迹碰撞比目鱼）。一般错误：protocol/delegate-stub 泛洪攻击 Swift/Java/C#/Go。

**方法论教训：** 当代理阅读小型存储库时，不要得出“采用地板”的结论 - diff
*它读取的内容*与工具*之前*返回的内容相对应。已阅读该工具的内容
给予=收养；工具返回**错误的东西**（存根端点、冲突名称）后的读取=
一个可修复的错误。文字记录推理（而不是中位数）告诉您这一点。强制 rustcodegraph-only
hook (block Read+Grep+Glob+Bash-search) 是单独确认充分性的无方差方法
来自收养。

---

## 4. 可重复的方法（按语言/框架运行）

### 第 1 步 — 选择框架的规范*流程*问题
每个框架都有一个签名数据/控制流。选择“X 如何到达/成为 Y”
问题和真实的回购协议（添加到 `.claude/skills/agent-eval/corpus.json`）。示例：
- React 状态→DOM，Vue 反应式→渲染，Svelte 存储→更新
- Rails请求→控制器→视图，Spring请求→`@Controller`→服务
- Express/Koa请求→中间件→处理程序，FastAPI请求→路由→依赖
- Redux操作→reducer→store，RxJS订阅→operator→observer
- 任何 ORM：查询构建器 → SQL 执行（django 模式）

### 第 2 步 — 测量漏洞（确定性，无代理）
```bash
rm -rf <repo>/.rustcodegraph && ( cd <repo> && rustcodegraph init -i )
rustcodegraph agent-eval probe-explore <repo> "<from-symbol> <to-symbol>"   # does the flow break? where?
rustcodegraph agent-eval probe-node <repo> <break-symbol>                   # trail: is the next hop missing?
```
“没有直接调用路径……在动态调度时中断”+中断处的稀疏路径
点**定位孔**（这正是 `_iterable_class` 和 `triggerUpdate` 的方式
被发现）。通过读取中断符号的主体来确认它是动态的。

### 第 3 步 — 分类 → 选择机制（使用 §2 表）
- `self.<attr>(...)` / 描述符 / 元类 → **解析器** (§3a)。
- `for(cb of store)cb()` / `store.forEach(cb=>cb())` → **现场观察合成器** (§3b)。
- `on('e',fn)` + `emit('e')` → **EventEmitter 合成器** (§3b)。
- 内联处理程序不是节点 → **命名：** 提取（通常已在
`tree_sitter.rs`）； **匿名：**合成器链接通过主体（尚未构建）。
- 不能作为一个类进行精确门控的调度（运行时键控的 `table[钥匙](...)`，
`getattr(self, expr)`、反射、类型化中介总线、`new Proxy`) → **边界
浮出水面**（`src/mcp/dynamic_boundaries.rs`，#687）：探索宣布调度
静态路径结束的站点 — file:line、form 和候选目标
键是静态可见的——而不是合成边缘。仅查询时间，零
图突变，仅当所询问的流无法连接时触发。这是
故意为边界设置地板：错误的边缘毒害了地图（沉默的节拍
错误），但诚实的“流量继续在这个网站上，可能进入这些
候选者”仍然保存了读取重建螺旋。当稍后形成边界时
证明在真实的存储库上可精确门控（例如相同的存储库文字键命令总线），
将其提升至合成器通道，边界音符自行消失 —
然后流程连接。

### 第 4 步 — 实施
- **解析器：** 添加到 `src/resolution/frameworks/<lang>.rs` — `resolve()` 分支 +
如果引用名称不是声明的符号，则为 `claims_reference(name)`。复制 Django 解析器模式。
- **合成器通道：** 扩展 `src/resolution/callback_synthesizer.rs` — 添加
框架的注册器/调度程序**名称模式**和**主体模式**（例如信号
使用`.connect()`/`.emit()`； Rx 使用 `.subscribe()`/`.next()`）。
- 重新索引（步骤 2 命令）并重新运行探索探针 — 流程现在应该已连接。

### 第 5 步 — 验证（每次都以相同的方式）
1. **确定性：** `rustcodegraph agent-eval probe-explore <repo> "<from> <to>"`
显示路径； `rustcodegraph agent-eval probe-node` 显示桥接跃点。这
先前中断的跃点已关闭。
2. **精度：** 计数 + 抽查合成/解析边缘 - 无爆炸，正确目标：
   ```bash
   sqlite3 <repo>/.rustcodegraph/rustcodegraph.db \
     "select s.name||' → '||t.name||'  '||coalesce(e.metadata,'') from edges e \
      join nodes s on e.source=s.id join nodes t on e.target=t.id where e.provenance='heuristic';"
   ```
（解析器边缘不是 `heuristic`；而是通过跟踪 + 被调用者进行验证。）
3. **回归：** 节点数稳定（`select count(*) from nodes;` 之前/之后 - 一个很大的
跳跃意味着提取变化过度）；控制仓库上的现有痕迹完好无损。
4. **端到端代理评估：**使用 rustcodegraph 和测量运行流程问题
**读取/答案完整性/成本**与修复前基线相比：
   ```bash
   # headless (exact cost + clean tool sequence)
   bash scripts/agent-eval/run-agent.sh <repo> with "<flow question>"
   # or the full A/B + interactive Explore-subagent path:
   scripts/agent-eval/audit.sh local <name> <url> "<flow question>" all
   ```
然后解析：`Read` 计数、rustcodegraph-tool 计数、成本以及现在是否有答案
包含粘合符号（之前需要读取的符号）。

### 成功标准（每种语言/框架）
- `rustcodegraph_explore` 端到端地呈现规范流（无动态调度中断）。
- 代理可以用 **Read 0** 回答流程问题（可在 ≥ 一些运行中实现）并且
答案中出现粘合符号。
- **没有节点爆炸**，并且控制仓库上没有回归。
- 抽查时合成的边缘是精确的（没有通用名称过度链接）。

---

## 5. 验证工具包（参考）

| 工具 | 目的 |
|---|---|
| `rustcodegraph agent-eval probe-explore <repo> "<from> <to>"` | 两个符号之间的调用路径（空洞检测器） |
| `rustcodegraph agent-eval probe-node <repo> <sym> [code]` | 符号+踪迹（调用者/被调用者）； `code` 添加本体 |
| `rustcodegraph agent-eval probe-explore <repo> "<query>"` | 探索输出 |
| `scripts/agent-eval/{audit,run-agent,itrun}.sh` | 代理 A/B（无头+交互式）；还有`/agent-eval`技能 |
| `sqlite3 <repo>/.rustcodegraph/rustcodegraph.db` | 直接边缘/节点检查（出处、元数据、计数） |

探测命令通过 `rustcodegraph agent-eval` 针对 Rust 二进制文件运行；以前的 `.mjs` 探头已退役。首先运行 `npm run build`。任何之后重新索引
提取或分辨率更改 (`rm -rf <repo>/.rustcodegraph && rustcodegraph init -i`) —
合成器/解析器在索引时间运行。测试夹具：为每个图案保留一个微小的夹具
（参见 `/tmp/cb-fixture/bus.js`；**运输时移至 `__tests__/`**）。

---

## 6. 覆盖范围矩阵（随意填写）

状态图例： ✅ 已完成+已验证 · 🔬 已识别漏洞 · ⬜ 未开始。
`Mechanism`：R = 旋转变压器，S = 合成器通道，X = 提取。

| 语言 | 框架 | 测试的规范流程 | 机制 | 地位 |
|---|---|---|---|---|
| COBOL | 静态 PERFORM / literal CALL / COPY | paragraph → program → program | X+R | 静态能力已实现；[覆盖与验收限制](#cobol-static-coverage)。九项外部验收已由用户取消，未作为通过项。 |
| VB.NET | 静态调用 / namespace Imports / 属性访问器体 | 属性 Get/Set → 跨文件方法 → 本地方法 | X+R | 静态能力已实现；[覆盖与验收限制](#vbnet-static-coverage)。九项外部验收已由用户取消，未作为通过项。 |
| Erlang | 静态调用 / 导入 / behaviour callback | 静态调用 → 模块函数；变量模块调用 → behaviour 实现 → 本地调用 | X+R+S | 静态能力与启发式桥接已实现；[覆盖与验收限制](#erlang-static-coverage)。九项外部验收已由用户取消，未作为通过项。 |
| Solidity | 静态调用 / 导入 / 继承 | 调用方 → 继承 modifier → 基类内部方法；导入别名 → 库函数 | X+R | 静态能力已实现；[覆盖与验收限制](#solidity-static-coverage)。九项外部验收已由用户取消，未作为通过项。 |
| Nix | 静态函数 / 模块加载 | 命名调用；相对 import / callPackage → 模块文件 → 声明 | X+R | 静态能力已实现；[九案例历史证据与验收范围](#nix-static-coverage)。外部验收已由用户取消，未作为通过项。 |
| TypeScript/JS | React / 观察者 / EventEmitter / React Router | 状态→渲染；调度→回调；路线→组件 | S+X | ✅ 渲染+调度（excalidraw）； **React Router JSX 路由** `<Route path component={C}/>` (v5) + `element={<C/>}` (v6) → 组件 (react-realworld **0→10, 10/10**)。 + **对象数据路由器** `createBrowserRouter([{path, element/Component}])`（文字形式）； Next.js config/`nextjs-pages` 误报已修复。 🔬 惰性数据路由器（`path: paths.x.path, lazy: () => import()` — 变量路径 + 惰性模块） |
| PHP | Laravel events | 调用方 → inline event(new Event) → handle → helper | X+S | 静态注册派发已实现；[覆盖与验收限制](#laravel-event-static-coverage)。九项外部验收已由用户取消，未作为通过项。 |
| C# | MediatR | 调用方 → inline Send/Publish → Handle → helper | X+S | 静态注册派发已实现；[覆盖与验收限制](#mediatr-static-coverage)。九项外部验收已由用户取消，未作为通过项。 |
| Ruby | Sidekiq | 调用方 → literal worker dispatch → perform → helper | X+S | 静态注册派发已实现；[覆盖与验收限制](#sidekiq-static-coverage)。九项外部验收已由用户取消，未作为通过项。 |
| Python | Celery | 调用方 → delay/apply_async 注册任务 → helper | X+S | 静态绑定派发已实现；[覆盖与验收限制](#celery-static-coverage)。九项外部验收已由用户取消，未作为通过项。 |
| TypeScript/JS | Pinia / Vuex stores | 实例调用 → Options action → helper；literal dispatch → namespaced action → commit → mutation → helper | X+S | 静态注册与派发已实现；[覆盖与验收限制](#vue-store-static-coverage)。九项外部验收已由用户取消，未作为通过项。 |
| TypeScript/JS | Vue/Nuxt | 模板事件（@click→处理程序）；成分组成；反应→渲染 | S+X | ✅ 事件+组合（vitepress S / vben M / element-plus L）； 🔬 反应式 → 渲染（vue-core 代理运行时 — 前沿，延迟） |
| TypeScript/JS | Svelte / SvelteKit | 模板调用/组合； SvelteKit 操作→api；存储→DOM | X | ✅ 已经很强大（现实世界S/骨架M/shadcn L）：模板`{fn()}`调用，`<Pascal/>`组合，`import * as api`命名空间，`load`→api所有开箱即用。 + 导出常量函数对象提取 (SvelteKit `actions`)。 🔬 `$lib`-来自操作的命名空间 + 存储/反应边界 |
| TypeScript/JS | 快递 / 相思木 | 请求 → 路由 → 处理程序 → 服务 | 右+X | ✅ 命名处理程序 + 中间件 + 控制器/服务（解析器） + **内联箭头处理程序 → 服务主体调用**（真实世界 S 19 / 解析 M / 幽灵 L 65 边）。 🔬 自定义路由器（有效负载有 0 条路由——不是 `app.get` 风格） |
| TypeScript/JS | NestJS | 请求 → @Controller → DI 服务 → 存储库 | 右 | ✅ 已经很好地覆盖了（现实世界 S / immich M-L / amplication L）：@decorator 通过解析器 + DI `this.svc.method()` 控制器路由（HTTP/GraphQL/微服务/WS）→服务大规模正确解析（名称+共置）。无动态调度孔。 🔬 提交的 `dist/` 构建输出被索引（现实世界） - 一般构建目录忽略后续操作 |
| TypeScript/JS | RxJS / 信号 | 订阅 → 操作者 → 观察者 | S | ⬜ |
| Python | Django ORM | 查询集 → SQL 编译器 | 右 | ✅ |
| Python | Django / DRF（视图） | url → 视图 → 模型 | 右+X | ✅ url→视图 (`path`/`url`/`as_view`) + **DRF `router.register`→ViewSet** (真实世界 S / wagtail M / saleor L); ORM 查询集→SQL（之前的工作）。 🔬 信号（`post_save`→接收器）、DRF 视图集 CRUD 操作（继承）、销售或 GraphQL 解析器 |
| Python | 烧瓶/FastAPI | 请求→路由→处理程序→依赖 | 右+X | ✅ **Flask：跨干预装饰器（`@login_required`）+堆叠`@x.route`行解析处理程序**（微博S 6→27，redash L装饰器路线6/6）； **FastAPI：空路径路由器根路由 `@router.get("")` 包括。多行** (现实世界 S 12→20 / Netflix 调度 L **290/290 100%**) + **裸名内置防护** - 以 Python 内置方法命名的处理程序 (`index`/`get`/`update`/`count`…) 被过滤为内置方法，并丢失了其路由→处理程序边缘。 + **Flask-RESTful `add_resource(Resource,'/x')` → 资源类** (redash 6→**77**) + **元组 `methods=('GET',)`** (被错误标记为 GET) + **扩大检测** (requirements/Pipfile/setup + subdir 应用程序工厂入口点 —flask-realworld 0→**19**)。 🔬 FastAPI `Depends()` 依赖边缘（轻验证） |
| 去 | Gin / chi / gorilla/mux / net-http | 请求→路由→处理程序→服务；中间件链（`Use`→`Next`） | S+X | ✅ **任何组变量**（`v1.GET`、`PublicGroup.GET`）上的路由，而不仅仅是 `r/router`（gin-vue-admin S→M 4→259 / realworld S / gitness L） - 缺少所有组路由应用程序；命名处理程序精确解析。 **gorilla/mux 确认由任何接收器 `HandleFunc`/`Handle` 处理覆盖**（子路由器变量 `s.HandleFunc(...)` + 命名空间处理程序；忽略 `.Methods()` 链）。 + **gin 中间件链合成器** (`ginMiddlewareChainEdges`)：gin 通过一条动态线运行其整个链 - `(*Context).Next` 运行 `c.handlers[c.索引](c)`，切片索引调度树托管者无法解析，因此 `callees(Next)` 在 `len()` 帮助器 (`safeInt8`) 处陷入死胡同，代理兔子洞重新查询它。找到调度程序（通过索引调用 `handlers` 切片的 Go 方法）并链接它 → 通过 `.Use`/`.GET`/…/`.Handle` 注册的每个 HandlerFunc；对现有的调度程序进行门控（在非 gin Go 存储库上是惰性的），仅命名处理程序（跳过闭包），有上限。 gin L：`callees(Next)` 现在表面 `Logger`/`Recovery`/`ErrorLogger`+ 处理程序（节点数稳定为 2,544；5 个精确边缘，带有 `registeredAt` 接线站点）。 **特工 A/B（无头中位数 4，Opus 4.8）：从 rustcodegraph 翻转杜松子酒 −58% 成本 / −129% 时间（兔子洞，包括在 2/4WITH 运行中的杂散 `Workflow` 失火）→ +7% 成本 / +35% 代币 / +8% 时间 / 38% 工具调用，所有 4 个WITH 运行干净（0 Read/Grep/Bash，无工作流程，无重复调用）。** 🔬 内联 `func(c){}` 处理程序（匿名，主体丢失）； subrouter/`PathPrefix` 路径前缀未预先添加（仅标签）；吉特尼斯气定制 (26/321) |
| 锈 | 阿克苏姆 / actix / 火箭 | 请求 → 路由 → 处理程序 | 右+X | ✅ **Axum 链式方法 + 命名空间处理程序** — `.route("/x", get(h1).post(h2))` 仅发出第一个方法+处理程序，而 `get(mod::handler)` 捕获模块而不是 fn (realworld-axum S **12→19, 19/19**)；平衡括号扫描+每个方法节点+最后一个`::`段处理程序。 **Rocket 属性宏 550/556 (99%)** (Rocket repo L) — 已经很强了。 crates.io 名为 axum 路由解析（6/8；其余是闭包/var 处理程序；其 API 主要是 utoipa `routes!` 宏 = frontier）。货物工作区模块解析（之前的工作）。 **actix 构建器 API** `web::resource("/x").route(web::get().to(h))` / `.to(h)` / 应用程序 `.route("/x", web::get().to(h))`（actix 示例 **51→128 条路线，35→112 已解析**） - 是占主导地位的 actix 风格，完全错过了（处理程序位于 `.to(h)`，而不是 `get(h)`）。 🔬 actix `web::scope("/api")` 前缀（不添加到嵌套资源路径前面）+ 匿名 `.to` 闭包处理程序 |
| 爪哇 | 春天 | 请求 → @RestController → @Autowired 服务 → 存储库 | 右+X | ✅ **裸 `@GetMapping`/`@PostMapping` + 类 `@RequestMapping` 前缀连接→路由→方法**（现实世界 S / 商场 M / 光环 L） - 缺少所有无路径方法映射； DI 控制器→服务解析（名称 + 目录）+ **接口→impl 调度合成器**（`interfaceOverrideEdges`：类的 `implements`/`extends` → 链接每个接口/基本方法→其同名覆盖；JVM 门控、上限、**过载感知**；mall **310** / halo **734** 合成边缘，节点数不变）因此跟踪如下控制器→服务**接口**→**实现**，而不是在抽象方法处死胡同 — `trace("PmsProductController.getList","PmsProductServiceImpl.list")` 在 **3 跳**中连接（探针验证）。 + **字段注入的具体 bean 跟踪** (#389)：`this.<field>.method()` 在提取时剥离 `this.` 接收器，解析器在封闭类的字段声明中查找接收器名称以获取声明的类型，然后解析其上的方法 - 当字段名称不大写为类型时关闭控制器→bean 跳跃（`@Resource(name="userBO") UserBO userbo` → `userbo.toLogin2()` 到达`UserBO.toLogin2`）。 + **`@Value("${k}")` / `@ConfigurationProperties(prefix="X")` → application.{yml,yaml,properties}** 与 Spring 的宽松绑定 (kebab↔camel↔snake) 绑定，包括。 `${k:default}`。 mall-tiny S：11/11 `@Value` 已解决。 ⚠️ **代理 A/B 为空**（n=2：代理进入上下文→探索→读取并且从未调用 `trace`，因此合成边缘没有被行使 - 采用门控，循环墙；参见 `docs/benchmarks/call-sequence-analysis.md`）。修复是正确的+无论如何都改进了跟踪/被调用者/影响/上下文连接；代理可见的读取减少需要跟踪采用。 🔬 Spring Data JPA 派生查询 (`findByEmail`) — 元编程前沿； `@PropertySource` 外部文件； Spring云配置；包之间的映射器类简单名称冲突（已删除以避免错误解析） |
| 爪哇 | MyBatis（XML 映射器） | DAO接口方法→`<select\|插入\|更新\|删除 id="X">` SQL | R（XML 提取）+ S（Java↔XML 合成器） | ✅ **XML 映射器作为一流语言** (#389) — `src/extraction/mybatis_extractor.rs` 解析包含 `<mapper namespace="...">` 的文件；每个语句发出一个方法形节点，限定 `<namespace>::<id>` + `<sql id="X">` 片段 + `<include refid>` 引用。非映射器 XML（pom、log4j）→ 仅文件节点。 `mybatis_java_xml_edges` 合成器通过 `<ClassName>::<methodName>` 索引 Java 方法，并通过后缀匹配连接到 XML 限定名称 — 消除了不明确的简单名称冲突（精度高于召回率）。 mall-tiny S **6/6 自定义 SQL 映射器方法桥接**至其 XML 语句；完整的企业链 `trace(controller.action → mapper.method-xml)` 跨控制器/service-iface/impl/mapper/XML 连接。 🔬 通过不合格的 refid 进行交叉映射器 `<include>`； MyBatis Plus动态方法（`BaseMapper<T>` CRUD继承自框架，不在项目中）；注解驱动的映射器（Java 方法上的 `@Select("SELECT ...")` — SQL 位于注解中，而不是 XML） |
| 科特林 | Spring Boot / Jetpack 组合 | 请求→@RestController→服务； @Composable → 子级 | 右+X | ✅ **Spring Boot Kotlin** — Spring 解析器仅是 `['java']`，带有 Java 语法方法正则表达式 (`public X name()`)；扩展到 `.kt` + Kotlin `fun name(` 处理程序匹配（petclinic-kotlin **0→18, 18/18**；类前缀连接；DI 控制器→repo 解析 — `showOwner ← GET /owners/{ownerId}` → `OwnerRepository.findById`）。 **Compose 组合已经是静态的**（@Composable→child 是普通函数调用 - Jetcaster `PodcastInformation→HtmlTextContainer`）。 Java Spring 不变（现实世界 19/19）。 🔬 Ktor `routing { get("/x"){…} }` lambda 处理程序（匿名）+ Compose 重组（隐式 `mutableStateOf`，无 setState 门）+ 协程/Flow |
| 迅速 | 汽 | 请求→路由→控制器 | 右+X | ✅ **每个真实应用程序上都有 0 条路由** — 提取器需要 `app/router/routes` 接收器 + `"path"` 文字，但分组构建器 (`let todos = routes.grouped("todos"); todos.get(use: index)`) 上的真实 Vapor 路由没有路径参数。重写：任何接收器、可选/非字符串路径段、`.grouped`/`.group{}` 前缀跟踪、`use:` 鉴别器。 steam-template S **0→3（3/3**，嵌套 `/todos/:todoID`），SteamPress M **0→27 (27/27)**，SwiftPackageIndex-Server L **0→14（14/14** 处理程序分辨率）。 🔬 类型化路由枚举（SPI `SiteURL.x.pathComponents` - 仅路径标签，处理程序仍然解析）+ 闭包处理程序 `app.get("x"){ }`（匿名） |
| 迅速 | Alamofire / 闭包集合 | 请求→构建→发送→**验证**（延迟关闭） | S | ✅ **闭包集合调度合成器** (`closure_collection_edges`)：Swift 延迟处理程序模式 `DataRequest.validate` `validators.write{$0.append(v)}` … 基本 `Request.didCompleteTask` `validators.forEach{$0()}` （在不同文件/类中追加 + 调度，字段为 `Protected<[() -> Void]>`）。元素调用 `$0(`/`it(` 是精度门 → **Alamofire 上有 9 个边**（验证器/流/finishHandlers/requestsToRetry），**每个非闭包集合控件上有 0 个边**。在 `trace` + 中内联显示，作为探索“动态调度链接”部分（因此当代理仅命名为 `validate`，而不是耗尽列表的 `didCompleteTask` 时，它会显示）。仅强制 rustcodegraph：**3/3** 构建/发送/验证正确。 + **跟踪端点相关性**：重载的 `request`/`task`（44/8 定义，大部分是空的 `EventMonitor` 委托存根）现在解析为真正的 `Session.request`，而不是 1 行无操作 — **WITH-arm 工具调用 12→8 中值，读取方差 0–12→1–4**（崩溃都是跟踪冲突）比目鱼）；控制安全（excalidraw/okhttp/gin 跟踪完整，gin A/B 0 读取）。 + **上帝文件多阶段渲染**（`format_explore_file_result`）：一个流程，其必要的代码跨越上帝文件（Session.swift构建链~11K）加上其他文件（验证逻辑），用于在固定的`maxOutputChars`处截断并删除最后一个阶段。六个协调层使其渲染所有阶段：（1）主干上帝文件将主干完整 + 路径外方法渲染为签名（真正的主干），（2）每个 NAMED 令牌的实质性定义都被播种到子图中（FTS 埋藏在构建条款下的 `validate` → Validation.swift 从未收集），（3）定义命名符号的文件优先于仅引用流的文件（Validation=50 >附带组合 = 23)，(4) 90% 预算早期突破和 (5) 总上限都免除必要的（命名/主干）文件 - 附带文件保持上限，(6) 最终上限为 1.5×，因此它不会分割循环组装的必要内容。 Alamofire 现在在 ONE Explore 中渲染 build+validators-exec+validate (~16K)； A/B读取med 2→**0.5**，工具8→**5.5**； excalidraw 控件保持在 0 读取（无膨胀）。顺序流脊柱是不可约的（没有多余的兄弟姐妹会崩溃）——解决方法是渲染它，而不是限制它。 |
| C# | ASP.NET 核心 | 请求 → [Http*] 操作 → DI 服务 → EF | X | ✅ **功能文件夹检测**（真实世界 0→19 — 未检测到）+ **裸 `[HttpGet]` + 类 `[Route]` 前缀**（eShopOnWeb 9→33 / jellyfin L） — 位于同一位置，因此不需要 `claims_reference`。 🔬 EF Core LINQ/DbSet（元编程前沿） |
| 红宝石 | 铁路/西纳特拉 | 请求→routes.rb→Controller#action→模型 | 右 | ✅ **RESTful `resources`/`resource` 路由→controller#action** (realworld S 16 / spree M / forem L)，复数 + only/ except + `claims_reference`；显式路由也固定为精确的 `controller#action`。 🔬 ActiveRecord 动态查找器 (`Article.find_by_slug`) — 元编程前沿 |
| PHP | 拉维尔 | 请求 → 路由 → 控制器 → Eloquent | 右 | ✅ **精确的 `Route::get([Ctrl::class,'m'])` / `'Ctrl@m'` → Ctrl@method** (realworld S / firefly M / bookstack L) - 将裸方法名称解析为错误的控制器（每个 `index`→ArticleController）；路线::资源→控制器。 🔬 雄辩的动态发现者/关系（元编程前沿） |
| PHP | 德鲁帕尔 | 请求 → *.routing.yml → _controller/_form | 右 | ✅ **FQCN 处理程序的 `claims_reference`** （`\Drupal\…\Class::method` 通过预过滤器只是因为 `::method` 名称已知；裸 `_form` FQCN `\…\FormClass` 和单冒号 `Class:method` 控制器服务在解析（）之前被删除）+ **单冒号控制器匹配** + **通过 Composer 检测 `type:drupal-*` / `name:drupal/*` + `*.info.yml` 后备**（未检测到带有空 `require` 的 contrib 模块 → 0 个路由）。 admin_toolbar S **0→14 (14/14)** / 网络表单 M 208 (**144**) / 核心 L 836 (536→**731, 87%**)。其余部分是**实体注释处理程序前沿**（`_entity_form: type.op` 通过实体的 PHP `#[ContentEntityType]` 处理程序解析，而不是直接类）。 🔬 **OOP `#[Hook]` 属性** — Drupal 11 将所有过程挂钩移至属性方法（核心：418 个 `#[Hook]` 文件与 3 个过程），因此解析器的 docblock/`module_hook` 检测对于现代核心来说已过时（0 个挂钩边缘） |
| C/C++ | C++ vtables/继承 | 虚拟呼叫→覆盖；一般直接调度 | S+X | ✅ **通用调度强**（redis C **29k** 跨文件调用 / leveldb C++ **1.4k**） + **C++ 继承提取修复**（`base_class_clause` 未处理，因此 C++ 扩展边缘丢失 - leveldb **219→298**） + **cpp 覆盖合成器**（基本虚拟方法 → 子类覆盖，门控到 C++，上限 - leveldb 12 精确： `Iterator::Next→MergingIterator`）。 🔬 C 回调结构（`s->fn()` → 422 路扇出，噪音太大而无法合成）+ C++ 纯虚拟基方法（`virtual void f()=0;` 声明不会提取为节点，因此这些覆盖无法桥接） |
| 镖 | 扑 | 设置状态 → 构建；构建 → 子部件 | S+X | ✅ **setState→构建合成器**（react-render 的 Dart 模拟：其主体调用 `setState(` → `build` 的 State 方法）门控到 `.dart` + **基础 Dart 方法范围修复** — Dart 将方法主体建模为签名的 *兄弟*，因此方法节点仅包含签名 (`end==start`)；现在 `endLine` 跨越了主体（所有主体分析都需要：被调用者、上下文切片、合成器的主体扫描）。柜台`initState→build`，书籍`build→BookDetail/BookForm`；小部件组成已经静态（compass_app `build→ErrorIndicator/HomeButton`）。控件保持不变（excalidraw 9,290 / django 302 - 范围修复仅扩展兄弟体语法）。 🔬 MVVM Command/ChangeNotifier 调度（compass_app — 无 setState）+ `Navigator.push(MaterialPageRoute(builder:))` 导航路线 |
| 卢阿/卢奥 | Neovim / Roblox | 模块调度(require→mod, mod.fn);事件/回调 | — | ✅ **已经涵盖了主要流程（测量优先，没有代码更改）** - Neovim 是重模块（`require('x')` + `x.fn()`），一般导入+名称解析已经处理它：telescope.nvim **220 导入 + 335 跨文件 `mod.fn` 调用**，端到端跟踪（`map_entries ← init.lua → get_current_picker (state.lua)`）。由提取器处理的 Luau 实例路径 `require(game:GetService(...))`。 🔬 事件回调注册（`vim.keymap.set(…, fn)`、autocmd `callback=`、Roblox `signal:Connect(fn)`）主要是内联匿名闭包（语料库 ~12 内联 vs ~2 命名）——匿名处理程序前沿；命名处理程序太罕见，无法证明合成器的合理性 |
| 斯卡拉 | 播放 / 阿卡 | 请求→conf/routes→控制器动作 | 右+X | ✅ **播放 `conf/routes` → 控制器** — 无扩展的 `conf/routes` 未编入索引；添加了窄文件遍历选择加入 (`isPlayRoutesFile`) + Play 解析器解析 `METHOD /path Controller.action(args)` → 操作方法（计算机数据库 **0→8, 7/8**；启动器 0→4, 3/4 — 未解析的是 Play 的框架 `Assets` 控制器，外部）。 Scala通用控制器→DAO调度已经解决。无回归：文件行走仅更改 ADDS Play 路线文件（excalidraw 9,290 / suite 800 不变）。 🔬 SIRD 编程路由器（代码中包含 `-> /v1 Router` + `case GET(p"/x")`）+ Akka actor `receive`/`Behaviors.receiveMessage` 消息→处理程序 |
| Swift × Objective-C | 混合 iOS 应用程序 | Swift `obj.foo(bar:)` → ObjC `-fooWithBar:`; ObjC `[obj fooWithBar:]` → Swift `@objc func foo(bar:)` | 右 | ✅ **Swift↔ObjC 跨语言桥** - `frameworks/swift-objc.ts` 实现 Apple 的 `@objc` 自动桥接名称数学（包括 init 形式 `initWith<First>:`、属性 getter+setter 对、`@objc(custom:)` 覆盖），并且反向去除 Cocoa 介词前缀(`With`/`For`/`By`/`In`/`On`/`At`/`From`/`To`/`Of`/`As`) 派生 Swift 基本名称候选。已在 Charts S **28/1 obj→swift / swift→objc**、realm-swift M **36/1185**、wikipedia-ios L **52/983** 上验证。通用名称块列表（`init`、`description`、`count`，...）保持精度。置信度 0.6（名称匹配的 1.0 获胜）- 仅当名称匹配没有结果时才会触发桥接。 🔬 ObjC 协议上的 Swift 泛型，ObjC 类上的 Swift 扩展（默默地错过；匹配 Java/Kotlin 泛型前沿） |
| JS × 原生 | React Native 遗留桥 | JS `NativeModules.X.fn(...)` → ObjC `RCT_EXPORT_METHOD` / Java/Kotlin `@ReactMethod` | 右 | ✅ **RN 遗留桥** - `frameworks/react-native.ts` 在 ObjC 端解析 `RCT_EXPORT_MODULE` （来自 `RCT` 前缀去除类名的默认名称）+ `RCT_EXPORT_METHOD(selector:(...))` + `RCT_REMAP_METHOD(jsName, selector)` ，在 Java/Kotlin 上解析 `@ReactMethod` + `getName()` 文字。 AsyncStorage S **8/8 精确**（`setItem`→`legacy_multiSet` 等），react-native-firebase L **`RCTEventEmitter` 内置阻止列表后为 18 精确**（初始 78 包括 60 个 `addListener:`/`remove:` 误报 - 每个发射器子类都通过 `RCT_EXPORT_METHOD` 声明这些误报，JS 调用者通过`NativeEventEmitter` 抽象不是直接的本机方法）。 🔬 动态桥键 (`NativeModules[someVar]`) — 仅文字键 |
| JS × 原生 | React Native TurboModules | JS 规范接口 ↔ 原生实现 | R（规范作为基本事实） | ✅ 部分 — 解析 `TurboModuleRegistry.get*<Spec>('Name')` + `Spec` 接口方法。每个规范方法通过选择器第一个关键字 (ObjC) / 标识符 (JVM) 与本机 impl 匹配。 react-native-svg S **9 精确**（`getTotalLength`、`getPointAtLength`、`getCTM`、`isPointInFill`，...）桥接到 Java impls（iOS 端是 Codegen 自动生成的，无需 `RCT_EXPORT_METHOD` 声明）。 🔬 不使用旧宏的 TurboModule 原生 impl 类（RNSvg iOS - 需要通过 Codegen 生成的 `NativeFooSpec` 超类进行继承感知桥接） |
| ObjC/Java/Kotlin → JS | React Native 事件发射器 | 原生 `sendEventWithName:`/`emit(...)` → JS `addListener('e', handler)` | S（跨语言通道） | ✅ **rn-event-channel 合成器** — 将 ObjC `sendEventWithName:@"X"`、Swift `sendEvent(withName: "X", ...)` 和 JVM `.emit("X", ...)` 与按文字事件名称键入的 JS `addListener('X', handler)` 匹配。与语言内通道相同的扇出上限 (`EVENT_FANOUT_CAP=6`)。 **RN 库 API (`const Foo = { watchX(listener) { addListener('e', listener) } }`) 的订阅包装器回退** — 当处理程序 arg 是参数时，回退到封闭函数，然后回退到封闭函数 `constant`/`variable`（对 JS API 表面的可达性正确归因）。 RNFirebase L **3 个推送通知流边缘**（UIApplicationDelegate → JS `onMessage`/`onNotificationOpenedApp`），RNGeolocation S **2 个位置事件边缘**（Swift `onLocationChange`/`onLocationError` → JS `Geolocation`）。 🔬 内联箭头处理程序 `addListener('e', d => …)`（匿名边界） |
| JS × Swift/Kotlin | 世博模块 | JS `requireNativeModule('X').fn(...)` → Swift/Kotlin `Function("fn") { ... }` | R（提取→合成方法节点） | ✅ **expo-modules 框架提取器** — 解析 Swift/Kotlin `Module { Name("X"); Function("y") { ... }; AsyncFunction("z") { ... }; Property("w") { ... } }` 文字并合成以每个声明命名的 `method` 节点。 JS 调用点通过现有的名称匹配器进行解析（不需要单独的 `resolve()`）。 expo-haptics S **6 个方法节点**（`notificationAsync`、`impactAsync`、`selectionAsync` × Swift + Kotlin）、expo-camera M **41**（完整的 SDK 表面，包括 `takePictureAsync`、`record`、`scanFromURLAsync`、视图道具 `width`/`height`）、expo SDK 扫描 L **134** （7 个包，72 个 Swift + 62 个 Kotlin）。包本身中的同名 JS 包装器会隐藏本机名称（`CameraView.tsx` 的 `pausePreview` 包装本机 `pausePreview`）；外部消费者应用程序直接桥接到本机。 🔬 闭包主体提取（Function 尾随闭包还不是主体范围节点） |
| JS × 原生 | React Native Fabric / Codegen + 遗留 Paper 视图组件 | JSX `<MyView prop={v}/>` → Codegen 规范 → 本机类（或 Paper `RCT_EXPORT_VIEW_PROPERTY` / `@ReactProp`） | R（提取）+ S（原生实现）+ JSX | ✅ **fabric-view 提取器 + Fabric-native-impl 合成器** — 提取器解析 **现代 Codegen TS 规范 (`codegenNativeComponent<NativeProps>('Name', ...)`) ** 和 ** 旧版 Paper 视图管理器宏（ObjC 上的 `RCT_EXPORT_VIEW_PROPERTY`，Java/Kotlin 上的 `@ReactProp`）。每个声明发出一个 `component` 节点 + 每个声明的 prop 发出一个 `property` 节点。合成器通过 RN 基于约定的名称+后缀 (`exact`/`View`/`ComponentView`/`Manager`/`ViewManager`) 将组件链接到其本机 impl 类。与`reactJsxChildEdges`结合，完整的消费者流程：JSX `<MyView/>`→布料`component`→原生类。在 RNSegmentedControl S **（旧版 Paper）1 个组件 + 11 个 props + 4 个桥**、RNScreens M **（纯 Codegen）27 个组件 + 272 个 props + 68 个桥**（第 6 阶段之前为 0）、RNskia L **（混合 + monorepo）5 + 14 + 15 上进行验证，跨 Codegen TS + Android Java + iOS ObjC**。添加了 **Monorepo 检测**：当根清单是工作区声明时，通过 `listDirectories` 探测 `packages/<sub>/package.json` 等（是 RNSkia 上的门控错误）。 🔬 Fabric 事件处理程序 props (`onTap={cb}`) — 需要 JSX 属性提取 |

（根据 `src/extraction/languages/` 验证确切的支持集和
开始之前的 `src/resolution/frameworks/` — 此表是一个起点。）

---

## 7. 已知限制和陷阱（来自 excalidraw/django 工作）

- **覆盖启用（但不强制）无读取路径。** 代理仍读取以*确认
来源*有时；成本保持平稳（rustcodegraph 调用交易读取）。可靠的
胜利是**完整性** + 使 Read-0 *成为可能*。不要指望成本一定会下降。
- **Vue（2026 年 5 月 23 日验证，vitepress S / vben M / element-plus L）。** SFC `<template>`
未被提取器解析，因此模板使用需要合成（`vueTemplateEdges`）：
`@click="fn"` → 处理程序，烤肉串 `<el-button>` → `ElButton`。 PascalCase `<Child/>` 是
已被 JSX 通道覆盖（SFC 组件节点跨越模板）。结果：
代理读取每个大小都会下降（vben 登录 1-3 与 4-11），**在处理程序所在的位置最强
本地函数** (vben `handleLogin`/`handleSubmit`)。
**可组合解构处理程序已解决：** `@click="closeSidebar"` 其中
`const { close: closeSidebar } = useSidebarControl()` 现在遵循别名 → 可组合 →
返回的 `close` fn （当它在可组合文件中定义时）。维特新闻侧边栏
流量下降**6 → 0 次读取**（最好情况）。仅限精确——没有可组合性的回退
本身（静态 `useX()` 调用边缘已经涵盖了这一点），因此它在
无法找到返回的 fn（例如重新导出/外部可组合）。剩余限制：
**前缀约定 kebab** — 元素加上 `el-button` → `button.vue` （组件名为
`button`，而不是 `ElButton`），所以烤肉串在那里仍未解决；和 **反应→渲染**
(vue-core 代理运行时) — 深层框架内部前沿，延迟。
- **Svelte / SvelteKit（于 2026 年 5 月 23 日验证，真实世界 S / 骷髅 M / shadcn L）— 已被充分覆盖。**
与 Vue 不同的是，`.svelte` 提取器已经解析了模板：`extractTemplateCalls` (`{fn()}`)，
`extractTemplateComponents`（`<Pascal/>` 合成 — 骨架 956 / shadcn 1610 参考边），
加上 `import * as api` 命名空间 + `load`→api 解析都可以工作。代理 A/B（现实世界登录）：
rustcodegraph **1 读** vs 没有 **4** — rustcodegraph 已经开箱即用。一个提取间隙
是**函数对象**（`export const actions = { default: async () => {} }`；步行者
故意跳过对象文字函数以避免内联对象噪音）。修复了 EXPORTED 常量
（一般 — Redux/Express 处理程序映射也是如此）； `extractFunction` `nameOverride` 保留内联对象箭头
跳过了。 **剩余：** 来自提取的操作节点的 `$lib` 别名命名空间调用 (`api.post`) 不会
解析，即使相同的别名解析为 `load` — 更深层次的解析器交互，延迟
（来自操作连接的本地/相对调用）。 **课程：在假设洞之前进行测量** — 现代 Svelte
几乎不使用 `on:click={fn}` （改为表单操作/回调道具），因此假设的事件处理程序漏洞
不是真的； Svelte 所需的资源远少于 Vue。
- **Express / Koa（2026 年 5 月 23 日验证，真实世界 S / 解析 M / Ghost L）——高价值内联处理程序修复。**
解析器已经处理了命名处理程序、中间件和 `XController.method`/`XService.method`。
真正的漏洞是**内联箭头路由处理程序**（`router.post('/x', async (req,res) => {...})` -
占主导地位的现代模式）：处理程序正则表达式 `[^)]+` 在箭头的 `)` 上中断，因此路线连接到
什么都没有，匿名处理程序的主体（请求→服务流）丢失了。整个内联处理程序
API 无法访问（现实世界 `POST /users/login` → 0 条边）。固定（`frameworks/express.ts`）：跨越
使用字符串感知平衡扫描进行调用；对于内联箭头，提取主体的调用（保留过滤到
删除 res/req/builtins) 并将它们归因于路由节点 → realworld **19** / Ghost **65** 精确
路由→服务边缘（POST /users/login→登录，POST /articles→createArticle，...），无节点爆炸，
框架范围（Express 之外的零爆炸半径）。 **确定性胜利是显而易见的；特工 A/B 浑浊
按仓库特征** - 现实世界（39 个文件）的大小低于 rustcodegraph 击败阅读的大小，并且
Ghost 的分层定制 API 架构让双臂都陷入困境。剩余：**自定义路由器** - 有效负载
6.4k 文件代码库有 0 个路由（其路由器抽象不是 `app.get` 样式，因此未被检测到）。课
Svelte 的反面：Express 的主导模式是未覆盖的模式，因此它需要像 Vue 这样的真正工作。
- **NestJS（2026 年 5 月 23 日验证，现实世界 S / immich M-L / 放大 L）- 已经被很好地覆盖。**
`nestjs` 解析器处理 @decorator 路由 (HTTP/GraphQL/microservice/WS)。 DI控制器→服务
（`this.svc.method()`）正确解析**即使在规模** - 每个immich控制器→服务边缘命中
正确的同模块服务（`addUsersToAlbum→addUsers`、`getMyApiKey→getMine`、`copyAsset→copy`）
名称+共置，不需要边的类型。 Agent A/B（immich专辑流程）：rustcodegraph **消除了Grep
(0 vs 3)** 追踪路由→控制器→服务。无动态调度孔。一个普遍的卫生差距浮出水面
（不是 NestJS 特定的）：现实世界的示例 **提交其 `dist/`** 构建输出，其中 rustcodegraph 索引
（246 个 dup 节点），因为文件遍历仅遵循 `.gitignore`，没有默认的构建目录忽略。真实的
apps (immich/ampplication) gitignore `dist/` (0 个重复节点)，所以它很窄 - 默认忽略
`dist/build/out/.next/coverage` 是一个干净的后续，延迟（核心索引器更改，用户的调用）。
- **Rails（2026 年 5 月 23 日验证，现实世界 S / spree M / forem L）——高价值的 RESTful 路由修复。**
`rails` 解析器只能看到显式的 `get '/x' => 'c#a'` 路由，因此资源路由应用程序（占主导地位的应用程序）
模式）有零个路线节点（现实世界+狂欢）。固定（`frameworks/ruby.ts`）：展开`resources :x` /
`resource :x` 进入其 RESTful 操作（仅/除了过滤器 + 单数 `resource` 的复数形式），
引用精确的 `controller#action`，并将其解析为 `<ctrl>_controller.rb` 中的操作方法
（显式路由也已修复——它们引用了一个模棱两可的 `action`）。现实世界**0→16**，前音
**0→635**精确路线→动作边缘。代理 A/B（前评论创建，大）：rustcodegraph **1–4 读取 /
0 grep / 47–53s** 与没有 **4–5 个读取 / 2–3 grep / 66–85s** — 读取更少，无 grep，速度更快。 **这
`claims_reference` 预过滤器是陷阱：** `articles#index` 名称没有声明的符号，因此解决
在 `resolve()` 运行之前删除它 - 需要与 django ORM 工作相同的声明钩子。残差：**导轨
引擎路由**（狂欢仍然为0——它安装了一个引擎，而不是`config/routes.rb`资源）；活动记录
动态查找器（`Article.find_by_slug` — 元编程前沿）。
- **Spring/MyBatis 企业流程（2026 年 5 月 26 日验证，mall-tiny S — 关闭 #389）。** 剩下的三个漏洞
规范的企业 Java 链（`HTTP 路由 → 控制器 → BO/Service → ServiceImpl → DAO/Mapper →
MyBatis XML SQL`）在真实的 Spring 项目中在多个跃点处被破坏。
  1. **字段注入的具体 bean 跟踪。** Java 的 `this.userbo.toLogin2()` 解析为 `method_inspiration(
object=field_access(this, userbo))`. The extractor surfaced `this.userbo.toLogin2` 逐字记录
名称匹配器的单点正则表达式无法解开它；即使有，`userbo` 也不能干净地大写
到 `UserBO` （`matchMethodCall.Strategy2` 中的 JVM 命名启发式），因此接收者类型的查找也
错过了。修复是在语言层，而不是 Spring 特定的：(a) 提取器解开 `field_access(this, X)`
使用 `X` 作为接收器（`src/extraction/tree_sitter.rs`）； (b) `matchMethodCall` 学会向上看
接收者名称作为封闭类中的字段声明，并使用该字段的 `signature` 存储
声明类型（`src/resolution/name_matcher.rs` 中的 `infer_java_field_receiver_type`）。重现确认
问题的确切示例：出现 `UserAction.toLogin2 → UserBO.toLogin2` 边缘（0 个传出边缘）。
  2. **MyBatis XML 映射器索引 + Java↔XML 桥。** `*.xml` 现在是一种语言 (`xml`)，具有自定义
提取器 (`src/extraction/mybatis_extractor.rs`)，每个 `<select|insert| 发出一个方法形状的节点
更新|删除|sql id="X">` qualified as `<命名空间>::<id>`, plus `<include refid="X"/>` → `<sql>`
片段参考。非映射器 XML（pom、log4j、web.xml）仅发出文件节点 — 无符号噪声。一个新的
合成器（`callback_synthesizer.rs` 中的 `mybatis_java_xml_edges`）通过以下方式索引 Java 方法
`<ClassName>::<methodName>` 并通过后缀匹配将它们连接到 XML 限定名称。模糊的
简单名称冲突被丢弃（精确度高于召回率）。 mall-tiny：6/6 个自定义 SQL 映射器方法
桥接到他们的 `<select>` 语句；全链 `trace(UmsRoleController.listResource → UmsResource
Mapper::getResourceListByRoleId(xml))` 通过控制器/服务/impl/mapper/XML 以 4 个跃点进行连接。
  3. **Spring 配置键链接。** `application.{yml,yaml,properties}` + 配置文件变体
（`application-dev.yml`、`bootstrap.yml` 等）在框架路径上解析。叶 YAML 键 + every
`.properties` 线成为由其虚线路径限定的 `constant` 节点。 `@Value("${k}")` /
`@Value("${k:default}")` 和 `@ConfigurationProperties(prefix="X")` 发出解析为的绑定节点
匹配的键（或者，对于前缀，其下最接近的键）。 **宽松的绑定**（烤肉串 `cache-list`
↔ 骆驼 `cacheList` ↔ 蛇 `cache_list` ↔ `CACHE_LIST`) 通过规范形式匹配处理。小型购物中心：
11/11 `@Value` 注释已解决（包括 `secure.ignored` `@ConfigurationProperties` 前缀）。
覆盖边界：跨模块 XML 语句引用（`<include refid="other.X">` 到中的片段）
另一个映射器文件 - 当包含使用点分命名空间形式时有效）； `@PropertySource` 外部
财产档案； Spring Cloud Config（远程属性）；包之间不明确的映射器名称冲突
（Java 映射器 `com.a.X` 和 `com.b.X` 均带有 `selectOne` — 目前已删除以避免错误解析）。
- **Rust 端口框架/动态调度奇偶校验（2026 年 6 月 21 日验证，合成小型装置）。** Rust
外观现在连接框架提取、本机解析器回退、引用解析和回调合成
对于 TypeScript 套件涵盖的相同代表性流程：Django/Flask 路由、Flutter
`setState→build`、C++ 类型指针调用和虚拟覆盖、JVM FQN 导入、Spring 字段/配置
绑定、MyBatis Java↔XML、React Native 桥/事件/Fabric、Drupal 路由、Java 匿名类覆盖、
和 Go gRPC generated-stub→handwriting-impl 调度。添加了新的 Rust 端 gRPC 覆盖范围
从 `Unimplemented*Server` 生成方法到独特手写的 `go-grpc-stub-impl` 启发式边缘
同名接收者方法，同时拒绝生成文件同级。验证：`frameworks_integration_test`
18/18、`rn_event_channel_test` 3/3、`react_native_bridge_test` 16/16、`fabric_view_test` 4/4、
`drupal_test` 30/30； Rust 奇偶校验忽略数下降至 318。
- **Rust 检索验证检查点（2026-06-21，macOS 本地 + 合成 MCP 探针）。** Rust 是
本地测试绿色（`cargo test -- --test-threads=1`；`cargo fmt --check`；Rust 忽略计数 318/318）。
两个文件的 TypeScript 固定装置确认了基本的 MCP 源检索和调用图连接
(`routeSave -> onSave -> processPayment -> settleInvoice`)，但 `rustcodegraph_explore` **还没有
与 TypeScript 相当的多符号流提示**：符号包查询仅返回探索
标题，而不是相关的来源/流程。代理 A/B 在验证策略下保持跳过状态，直到
探索流程达到同等水平。 Linux Docker 被无响应的 Docker 守护进程阻止； Windows虚拟机
验证因缺少 `.parallels` 连接文件而被阻止。
- **Spring（2026 年 5 月 23 日验证，真实世界 S / mall M / halo L）——裸映射 + 类前缀路由修复。**
解析器需要映射正则表达式中的字符串路径，因此 BARE 方法映射（`@PostMapping` 与
类 `@RequestMapping` 上的路径）——主要的多方法控制器模式——被错过了（晕
有28条路线，2444个文件；现实世界最喜欢的 2 动作控制器仅链接一个）。使固定
(`frameworks/java.ts`)：将类 `@RequestMapping` 视为前缀（已加入，而不是虚假路由）；匹配
特定于动词的映射 BARE-or-with-path；还处理方法级 `@RequestMapping(method=...)` （较旧的
风格）。现实世界13→19，商城→246条精准路线→方法（加入类前缀）； DI控制器→服务
解决（`article→findBySlug`）。代理 A/B（商城购物车流程）：使用 rustcodegraph 0 读取/0 grep 与不使用 2/2。
**通过删除 `@RequestMapping`-on-method 首次切割回归购物中心 292→1** — *被交叉回购捕获
路线计数检查*；剧本中的回归守卫是值得的。 Residuals：光环的自定义图案
（9/29 决议）； Spring Data JPA 派生查询（元编程前沿）。
- **Django / DRF（2026-05-23 验证，现实世界 S / wagtail M / saleor L） - 大部分覆盖 + DRF 路由器
修复。** ORM（`_iterable_class`→ModelIterable，原始调查）和 URL 路由
（`path`/`url`/`as_view`→查看）已经完成。第一个漏洞： **DRF `router.register(r'articles',
ArticleViewSet)`** (the core CRUD endpoints) wasn't extracted — only `path()`/`url()` 是。使固定
(`frameworks/python.rs`)：匹配 `router.register`（STRING 第一个参数将其与
`admin.register(Model, Admin)`，其第一个参数是模型类）→路线→ViewSet类。狭隘于此
语料库（现实世界有 1 个路由器；wagtail 使用 `path()`，saleor 是 GraphQL），但对于 DRF 路由器 API 来说是真实的。
代理 A/B（wagtail 页面流，中）：rustcodegraph **4–7 个读取 / 1–4 grep / 58–81s** 对比没有 **7–9 个读取
/ 6 grep / 82–86s** — 更少的读取，更少的 grep，更快。无回归（鹡鸰/卖家路线计数
不变——纯累加）。残差：信号（`post_save`→接收器），DRF视图集CRUD操作
（继承自基类，不在用户的ViewSet中），saleor的GraphQL解析器。
- **Laravel（2026 年 5 月 23 日验证，realworld S / firefly M / bookstack L）- 路线精度修复。**
解析器从处理程序中丢弃了控制器：`Route::get([UserController::class,'index'])` /
`'UserController@index'` 发出了一个 BARE `index` 引用，该引用的名称匹配错误地解析为 WRONG
控制器（每个 `index`/`show` → 以最先找到的为准；现实世界的 GET 用户 → ArticleController.index，
应该是用户控制器）。修复（`frameworks/laravel.ts`）：发出精确的`Controller@method`（数组+字符串
语法，命名空间剥离）+ `claims_reference` 它通过了预过滤器 → 现有的 Pattern-4
`resolveControllerMethod`。现实世界中所有路线都是正确的；书架 267/332 精确（获取页面 →
PageApiController.list）。代理 A/B（书库页面视图，大）：rustcodegraph **2–3 个读取 / 1–2 个 grep /
51–60 秒** 对比没有 **4–6 / 3–5 / 60–74 秒**。没有节点爆炸。残差：萤火虫仅解析 3/568
（其流畅的 `->uses()` / `['uses'=>...]` 处理程序格式未解析）；雄辩的动态发现者
（元编程前沿）。
- **Gin / chi（2026-05-23 验证，realworld S / gin-vue-admin M / gitness L）- group-var 路由修复。**
路由正则表达式仅匹配 `(router|r|mux|app|e).METHOD(...)`，但实际应用程序在 GROUP vars 上路由
（`v1.GET`、`PublicGroup.GET`、`userRouter.POST`），因此组路由应用程序几乎没有连接
（gin-vue-admin：**4 个路由用于 625 个文件**）。修复（`frameworks/go.ts`）：将接收器扩展到任意
标识符 — 动词 + 字符串路径 + 处理程序参数门使其保持特定于路由（`http.Get(url)` 没有
处理程序 arg → 排除）。 gin-vue-admin **4→259** 路由（257 条精确解析：`POST createInfo →
创建信息`);现实世界稳定（无回归）；没有垃圾。 **代理 A/B（创建用户流程）：rustcodegraph
0 次读取 / 0 grep / 26–30 秒 vs 没有 3 / 3 / 52–53 秒 — 迄今为止最干净的后端胜利（0/0，快 2 倍）。**
残差：内联 `func(c *gin.Context){}` 处理程序（匿名，主体丢失 - 就像修复之前的 Express）；
gitness 的 chi 自定义处理程序 (26/321)。
- **ASP.NET Core（2026 年 5 月 23 日验证，真实世界 S / eShopOnWeb M / jellyfin L）— 检测 + 裸属性
修复。** 两个漏洞：(1) `detect()` 仅在 `/Controllers/` 目录或根 `Program.cs`/`.csproj` 上触发（其中
通常不在索引源集中），因此功能文件夹应用程序（现实世界：`Features/*/FooController.cs`，
尽管设置了完整的控制器，但从未检测到子目录 `Program.cs`） → 0 条路由。扩大：扫描
用于 ASP.NET 签名的控制器/程序/启动 `.cs`。 (2) 属性正则表达式需要字符串路径→
裸 `[HttpGet]`（`[Route("[controller]")]` 类上的路线）错过了（eShopOnWeb 为 24 裸/2
细绳）。匹配 bare-or-path + 加入类 `[Route]` 前缀（如 Spring）。 **没有`claims_reference`
需要** - ASP.NET 属性路由与操作位于控制器中，因此裸方法
ref 解析同一文件（与 Rails/Laravel 不同，它们的路由位于单独的文件中）。现实世界0→19，
eShopOnWeb 9→33，jellyfin 362→399，全部精确（`GET /articles → Get`，加入类前缀），无爆炸。
代理 A/B（eShop 目录列表）：rustcodegraph **1–2 读取 / 0 grep / 63–75s** 对比没有 **6–7 / 1–6 /
77–79 秒**。剩余：EF Core LINQ/DbSet（元编程前沿）。
- **Flask / FastAPI（2026-05-23 验证，fastapi-realworld S / Flask-microblog S / Netflix 调度 L /
redash L) — 装饰器提取 + 内置名称修复。** 路由已提取，但请求→路由→处理程序
流程在两个正则表达式假设和一个解析器过滤器处中断。 (1) **之后立即需要烧瓶 `def`
`@x.route(...)`**，因此任何中间装饰器（`@login_required`、`@cache.cached`）或**堆叠的 `@x.route`
lines**（一个视图绑定到多个 URL）丢弃了路线 - 微博提取了 27 条真实路线中的 **6 条。
将Flask切换为FastAPI的`findHandler`扫描（匹配装饰器，然后找到下一个`def`），跳过
介入装饰器：**6→27**，全部解决。 (2) **FastAPI 的路径正则表达式 `[^'"]+` 拒绝了空路径**
`@router.get("")`（路由器/前缀根路由，经常是多线）→现实世界丢失了8个端点（列出/创建
文章、评论、登录/注册）。 `[^'"]+`→`[^'"]*` + 空路径名称保护：真实世界 **12→20**，Netflix
调度 **290/290 (100%)**。 (3) **裸名内置防护** (`src/resolution/index.rs`)：名为的处理程序
在Python内置*方法*（`index`、`get`、`update`、`count`…）被`isBuiltInOrExternal`过滤后
并失去了它的路由→处理程序边缘 - 微博的 `index` 视图（其 `/` + `/index` 堆叠路由）解析为
没有什么。点方法分支已经有一个 `knownNames` 保护；将其镜像到光秃秃的树枝上（一个名字
声明的符号拥有不是内置调用）。 +2 现实世界中的合法边缘，** django 控件上的 0 变化**
（302/373 相同 - 保持精度）。端到端的流跟踪（`login → get_user_by_email` 2 跳；
`create_user → from_dict`）。代理 A/B（现实世界登录-身份验证流程，n=2/arm）：rustcodegraph **0–1 read / 0 grep /
3–4 rustcodegraph / 30–39s**（上下文→[搜索]→trace→节点）与没有 **3 read / 2 grep / 33–36s** — 消除
grep，将读数削减为 0-1（小型仓库，因此挂钟平局；工具数量下降是胜利）。残差： **Flask-RESTful** 基于类
`api.add_resource(Resource,'/x')`（redash 的实际 API 形状 - 一个单独的类方法作为动词机制，而不是
自述文件记录的装饰器/蓝图 Flask）和预先存在的 **JS 文件路由误报**
redash 的 React 前端（来自 JS 解析器的 32 个伪造的 `.js`“路由”——与 Python 无关）。 **课程：
内置名称过滤器是 Python** 中的无声精度税 - 任何名为 `get`/`index`/`update` 的视图​​/函数
失去边缘；该修复是通用的（也有助于 Django/DRF 处理程序），而不是 Flask 特定的。
- **Drupal（2026 年 5 月 23 日验证，admin_toolbar S / webform M / drupal-core L） - 预过滤 + 检测修复。**
`*.routing.yml` 提取器和 `_controller`/`_form` 解析器已经存在，但仍存在两个差距
路线未链接。 (1) **`claims_reference` 预过滤器陷阱（再次）：** Drupal 处理程序引用是 FQCN
(`\Drupal\…\Class::method`)、裸形式类 (`\…\SettingsForm`) 或单冒号控制器服务
(`\…\Controller:method`)。只有 `::method` 形状在 `resolveOne` 的预过滤器中幸存下来（其 `member` 是
已知方法名称）；裸 FQCN 形式和单冒号控制器未命名任何声明的符号，并且是
在 `resolve()` 运行之前下降。添加了 `claims_reference` (FQCN / `Class:method` / `hook_*`) + 单冒号
控制器正则表达式中的分支 → 核心 **536→836 条路由中的 731 (87%)**；现在所有三个先前破碎的形状
解析（`/admin/content/comment`→CommentAdminOverview 表单，`/big_pipe/no-js`→setNoJsCookie 控制器）。
(2) **检测错过独立贡献模块：** `detect()` 只检查了 Composer `require` 的 a
`drupal/*` dep，但 contrib 模块通常有一个 EMPTY `require` 并且仅由
`"name":"drupal/<m>"` + `"type":"drupal-module"`（管理工具栏 → 0 条路线）。扩展到作曲家姓名/类型
  + `*.info.yml` 后备 → admin_toolbar **0→14 (14/14)**。规范流遍历 (`getAnnouncements` ←
`/admin/announcements_feed`）；节点数不变（仅分辨率）。代理 A/B（dblog 路由→控制器，
n=2/arm): rustcodegraph **0 read / 1 grep / 20–22s** 与没有 **1 read / 2 grep + glob / 28–32s** — 更少
工具，并且在约 10k 文件核心上速度更快。 **残差（前沿）：**
实体注释处理程序（`_entity_form: comment.default` → 在实体的
`#[ContentEntityType]` 注释，不是直接引用 — 核心的 ~78 部分 ~105 尚未解决）和 **OOP
`#[Hook]` 属性** — Drupal 11 将几乎所有过程挂钩转换为 `#[Hook('event')]` 方法（核心：
418 个属性文件 vs 3 个过程 `*.module` 挂钩），因此解析器的过程挂钩检测（docblock
`@Implements` / `module_hook` 命名）在现代核心中基本上找不到任何内容（0 钩边）。两者都是真实的
后续行动，而不是回归。
- **Rust / Axum + Rocket + actix（2026 年 5 月 23 日验证，realworld-axum S / actix-examples + Rocket M / crates.io L）——Axum 链式方法 + 命名空间处理程序修复。**
属性宏路径（`#[get("/x")] fn h`、actix/Rocket）和单个 Axum `.route("/x", get(h))` 已经
有效，但 Axum 提取器使用平面正则表达式，仅捕获路线的第一个 `method(handler)`
并且只有一个裸露的 `\w+` 处理程序。两个占主导地位的阿克苏姆习语打破了它：（1）**方法链**
`.route("/user", get(get_current_user).put(update_user))` — `.put` 臂没有生成路线节点，所以一半
缺少 API（realworld-axum 只有每个链的 GET）； (2) **命名空间处理程序**
`get(listing::feed_articles)` — `\w+` 捕获了 `listing`（模块），因此该路由解析为空。
使用每个 `.route(...)` 调用、每个方法节点和最后一个 `::` 段的平衡括号扫描进行重写
处理程序名称 → realworld-axum **12→19 条路线，19/19 已解决**（现在存在每个链接的 PUT/DELETE/POST；
`feed_articles` 解决）。 **火箭不需要什么**（550/556，99% - 属性宏）。 crates.io 确认
命名空间 axum 处理程序解析 (router.rs 6/6)，但通过 `utoipa_axum` `routes!` 定义其大部分 API
宏（前沿）并具有 SvelteKit 前端（其 50 条“路线”中的 42 条是 `+page.svelte`，正确地说
归因于 SvelteKit）。代理 A/B（更新用户流程，
n=2/arm): rustcodegraph **0–2 read / 0 grep / 32–40s** 与没有 **3 read / 0–1 grep + glob / 33–41s** — 适度
（realworld-axum 位于小型仓库绑定区域）但一致，具有一次完全干净的 0-read/0-grep 运行。节点
计数稳定； Axum 修复是 Axum 范围内的（attribute/actix/Rocket 路径未受影响）。
- **Actix 运行时路由（2026 年 5 月 23 日验证，actix-examples）——构建器 API 是主导风格，完全被错过。**
Actix 的属性宏 (`#[get("/x")] fn h`) 已被覆盖，但真正的 actix 应用程序通过构建器 API 进行路由：
`web::resource("/path").route(web::get().to(handler))`、`web::resource("/").to(handler)`（所有方法）和
应用级`.route("/path", web::get().to(handler))`。处理程序位于 `.to(handler)`，而不是 `get(handler)`，
因此 Axum `.route` 扫描没有为他们提取任何内容 - actix-examples 有 **80 `web::resource` 调用** 所有
未链接。添加了一个 actix 块：扫描每个 `web::resource("/path")`（在下一个边界处限制其方法链）
对于 `web::METHOD().to(h)` 对，回退到直接 `.to(h)`（方法 `ANY`），加上
应用程序级 `.route("/x", web::METHOD().to(h))` 形式。 actix-examples **51→128 条路线，35→112 条已解决
(87.5%)** (`GET /user/{name}`→with_param, `POST /user`→add_user)。 Axum 上没有回归（现实世界-axum 仍然
19/19) — actix 模式 (`web::resource`/`web::method().to()`) 不会出现在 Axum 代码中。 **残差
（前沿）：** `web::scope("/api")` 前缀不会添加到嵌套资源路径和匿名 `.to(|req|
...)` 闭包处理程序没有命名目标（~16 个仍未解决）。
- **Swift / Vapor（2026 年 5 月 23 日验证，vapor-template S / SteamPress M / SwiftPackageIndex-Server L） - 解析器在真实应用程序上实际上已失效。**
Vapor提取器仅匹配`(app|router|routes).METHOD("path", use: handler)`，但现代Vapor路线
在 `RouteCollection.boot(routes:)` 内的分组构建器上： `let todos = paths.grouped("todos");
todos.get(use: index)` — 任何 var 接收器，无路径 arg（路径是组前缀）。每个真实的应用程序都经过测试
提取**0条路线**（模板、penny-bot、Feather、SteamPress、SPI）。重写提取器：（1）任意
接收器 `\w+`（不仅仅是应用程序/路由器/路由）； (2) 可选的路径段，可以是非字符串
(`User.parameter`, `:id`, 路径常量) — `use:` 关键字是将路由与
`Environment.get("X")` / `req.parameters.get("X")`； (3) 来自 `let X = Y.grouped("a")` 的组前缀映射和
`Y.group("a") { X in }` 因此分组/嵌套变量上的路由获得完整路径（`todo.delete(use: delete)` →
`DELETE /todos/:todoID`）。结果：vapor-template **0→3（3/3**，嵌套路径精确），SteamPress **0→27
（27/27**，包括 `BlogPost.parameter` 路由），SPI **0→14（14/14** 处理程序分辨率）。规范流
遍历 (`createPostHandler` ← `GET /createPost`, → `createPostView`)。 **残差（前沿）：**
类型化路由枚举（SPI 寄存器通过 `app.get(SiteURL.x.pathComponents, use:)` — 处理程序解析，但
路径标签是 `/`，没有字符串文字）和闭包处理程序（`app.get("hello") { req in }` - 匿名，没有
命名目标）。 penny-bot（Discord bot）和 Feather（自定义模块路由器）在以下位置没有标准 Vapor 路由
所有 - Vapor 生态系统的路由风格差异很大。代理 A/B（创建发布流程，n=2/arm）：rustcodegraph
**0 read / 0 grep / 4 rustcodegraph / 26–30s**（两者都完全干净地运行）与没有 **1–4 read / 0–2 grep +
glob/bash，一次运行产生一个子代理 / 34–48s**。节点数量稳定；修复是 Vapor 范围内的（SwiftUI/UIKit
未触及）。
- **React Router 路由（2026 年 5 月 23 日验证，react-realworld S）——React 行的路由一半。**
React 渲染（state→render、jsx-child）已经涵盖；路线→组件不是 - `react.ts` 提取
Components/hooks 和 Next.js 文件路由但返回 `references: []`，因此生成了 `<Route>` 声明
没有什么。添加了 `<Route>` JSX 提取：在每个 `<Route\b` 之后扫描一个窗口（因此嵌套的 `>`
`element={<Comp/>}` 不会截断它），将 `path="…"` + `component={C}` (v5) 或 `element={<C/>}` (v6) 拉入
任何属性顺序，发出路由节点 + 组件引用（通过现有的 PascalCase 解析
`resolveComponent`）。 React-realworld **0→10, 10/10** (`/login`→登录，`/editor/:slug`→编辑器，
`/@:username`→轮廓);通过 `\b` 边界排除 `<Routes>` 容器。 exalidraw 没有回归
（9,290 个节点，46 个反应渲染合成边完好无损，0 个错误路由）。 🔬 对象 **数据路由器** API
`createBrowserRouter([{ path, element }])`（现代 v6，由Bulletproof-react 使用）是基于对象的，而不是 JSX —
单独的边界；加上预先存在的 Next.js 误报（`pages/` 应用程序目录中的 `*.config.mjs` 已处理）
作为路线）。
- **Dart / Flutter（2026-05-23 验证，flutter/samples：counter S / books S / compass_app M） - 合成器 + 基础提取器修复。**
Flutter 的反应跳跃是 `setState(() {…})` 重新运行 `build(context)` — 框架内部，没有静态边缘，
所以“tap → handler → setState → rebuilt UI”在 setState 处陷入死胡同（React 的 setState→render 的 Dart 模拟）。
添加了 `flutter-build` 合成器通道（阶段 4b）：对于具有 `build` 方法的每个 Dart 类，链接每个
同级方法，其主体调用 `setState(` → `build`（门控到 `.dart`）。 **但它被阻止了
基本差距：** Dart 将方法体建模为 `method_signature` 节点的*同级*，因此每个 Dart
方法节点有 `endLine == startLine` （仅签名） - `sliceLines(start,end)` 只看到 `void f() {`，从来没有
身体。修复了共享 `createNode`：当函数/方法的解析主体位于节点之外时，
将 `endLine` 扩展到它（受保护 - 子体语法是无操作；控制 excalidraw 9,290 / django 302
不变）。这个修复是基础性的，不是 Flutter 特有的——每个 Dart 被调用者/上下文/主体扫描都是
之前被截断。结果：计数器 `initState→build`，预订 `initState→build` + `build→BookDetail/BookForm`。
**小部件组合不需要综合** - 与 JSX 不同，Dart 小部件是显式构造函数调用
(`BookDetail(...)`)，已经静态(compass_app `build→ErrorIndicator/HomeButton/_Card`)。 **残差
(前沿):** MVVM状态管理(compass_app使用Command/ChangeNotifier + ListenableBuilder, 0 setState —
不同的调度形状）和 `Navigator.push(MaterialPageRoute(builder: (_) => DetailPage()))` 导航
（路线作为小部件，未覆盖）。
- **Kotlin / Spring Boot + Jetpack Compose（2026 年 5 月 23 日验证，spring-petclinic-kotlin S / compose-samples）——将 Spring 扩展到 Kotlin；撰写是免费的。**
Kotlin 的框架覆盖率为零——没有列出 `kotlin` 解析器，Spring 解析器是“语言：
['java']` with a `.java`-only extract gate and a Java-syntax handler regex (`public X name()`)。所以Spring Boot
Kotlin 应用程序（相同的 `@GetMapping`/`@RestController` 注释、`.kt` 文件）提取了 0 个路由。扩展
Spring 解析器：`['java','kotlin']`，接受 `.kt`，并添加一个 Kotlin `fun name(` 替代方案
处理程序方法正则表达式（Kotlin 没有访问修饰符，返回类型位于名称后面）。 petclinic-kotlin
**0→18, 18/18**;类 `@RequestMapping` 前缀连接，堆叠注释 (`@ResponseBody`) 被跳过，DI
控制器→repo解析（`showOwner ← GET /owners/{ownerId}` → `OwnerRepository.findById` /
`VisitRepository.findByPetId`）。 Java Spring 不变（现实世界 19/19 — Kotlin `fun` 和 Java `public X`
每种语言的替代方案都是不相交的）。 **Jetpack Compose 组合不需要工作** — `@Composable`
调用子 `@Composable` 的函数是普通的 Kotlin 函数调用，已经是静态的（Jetcaster
`PodcastInformation→HtmlTextContainer`、`FollowedPodcastCarouselItem→PodcastImage`)，如 Dart 小部件
构造函数。代理 A/B（视图所有者流程，n=2/arm）： rustcodegraph **0–1 read / 0 grep / 1 rustcodegraph / 11–18s** （a
单个 `context` 调用即可回答它）与没有 **2 read / 0–1 grep + glob / 20–28s** 相比。 **残差（前沿）：**
Ktor `routing { get("/x") { … } }` 内联 lambda 处理程序（匿名、
无命名目标），Compose 重组（隐式 — 读取 `mutableStateOf` 触发重组，无
`setState` 风格的门来锚定合成器），以及协程/流程调度。
- **Lua / Luau（2026 年 5 月 23 日验证，telescope.nvim / lualine.nvim / Knit — 测量优先，已涵盖）。**
矩阵猜测“事件/回调调度（合成器）”，但测量结果却不然：真正的 Neovim
插件是 MODULE-dispatch-heavy (`local m = require('telescope.actions'); m.fn()`)，而 rustcodegraph 的一般
`require`-导入 + 跨文件名解析已经处理它 - Telescope.nvim 有 **220 个已解析的导入
和335个跨文件`module.fn`调用edges**，并且一个流程跟踪端到端（`map_entries ← init.lua →
get_current_picker` 在 actions/state.lua 中）。 Luau 提取器已经处理 Roblox 实例路径要求
(`require(game:GetService("ReplicatedStorage").Packages.Knit)`)。 **假设的洞不是真实的** - 就像
Svelte/NestJS。真正的前沿是事件回调注册（`vim.keymap.set(mode, lhs, fn)`，autocmd
`{callback=fn}`、Roblox `signal:Connect(fn)`），但它主要是内联匿名闭包（语料库：~12
内联 `:Connect(function…)` 与 ~2 命名），望远镜的键盘映射是内联函数或 vim 命令
字符串，未命名参考。仅命名回调合成器将覆盖一小部分，因此根据“之前的测量”
构建/部分覆盖比没有更糟糕”，没有构建 - 没有代码更改；记录为已验证。
代理 A/B（actions.utils 地图流程，n=2/arm）：rustcodegraph **0 读 / 0 grep / 18–24s** 对比没有 **1 读
(+glob) / 24–25s** — 小流量如此适度，但 0 读取确认模块调度是可导航的。
- **Scala / Play（2026 年 5 月 23 日验证，播放样本：计算机数据库/启动器/rest-api）- 播放 conf/routes → 控制器。**
Scala 的通用调度（控制器→DAO）已经解析，但是 Play 在 EXTENSIONLESS 中声明路由
`conf/routes` 文件 (`GET /computers controllers.Application.list(p: Int ?= 0)`) 文件行走从未索引
（`isSourceFile` 需要扩展）。添加了狭窄的选择加入（`isPlayRoutesFile`：`conf/routes` / `*.routes`）
通过无语法（yaml 样式）路径路由，加上解析每个路径的 Play 解析器
`METHOD /path Controller.action(args)` 行（删除包前缀 + 参数）并解析 `Controller.action`
到该控制器类中的操作方法。计算机数据库 **0→8 条路线，7/8** （未解决的 1 条是
`controllers.Assets.versioned` — Play 的框架资源控制器，外部），启动器 0→4 (3/4)。流量
连接请求→路由→控制器→DAO。 A/B（列表计算机，n=2/arm）：rustcodegraph **0 read / 0 grep / 3
rustcodegraph / 17–22s** 对比没有 **2–3 read / 1–2 grep + glob / 16–17s**。 **无回归：** 文件遍历
仅更改 ADDS Play 路线文件（窄匹配）- excalidraw 9,290 和全套 (800) 不变。
**残差（前沿）：** 玩 SIRD 编程路由器（`-> /v1 v1.PostRouter` 包括 + `case GET(p"/x")`
在 Router 类中 —rest-api-example）和 Akka actor 消息→处理程序（`receive { case Msg => … }` /
`Behaviors.receiveMessage` — 无类型，合成器形状）。
- **C / C++（2026-05-23 验证，redis C / leveldb C++）——一般调度工作； C++ 继承修复 + 覆盖桥。**
测量优先：C/C++ DIRECT 调度非常出色，开箱即用（redis **29,464 跨文件调用边缘**，
leveldb **1,462**) — 值的大部分。动态调度前沿有两种形状： (1) C 回调
结构体 (`struct {.proc=fn}` + `cmd->proc()`) — 但在 Redis 中，`proc` 字段扇出到 **422** 命令
函数，噪音太大而无法精确合成，因此故意跳过（每个“部分覆盖比
无”）。（2）C++ vtable（`iter->Next()` → 子类覆盖）。覆盖链接被上游阻止：
`extractInheritance` 处理 `base_clause` (PHP)，但不处理 C++ 的 `base_class_clause`，因此 C++ `extends` 边缘
缺失/部分（修复后leveldb 219→**298**）。添加了 `cpp-override` 合成器通道（C++
类似于react-render）：对于每个`extends`边，链接每个基本方法→相同的子类方法
名称，以便跟踪/被调用者从接口方法到达实现。 leveldb **12条精确边缘**
(`Iterator::Next/Seek/Prev → MergingIterator`)，C (redis) 和 TS 上为 0（excalidraw — 门控到 C++）； C++
覆盖集成测试通过。 **剩余（前沿）：**纯虚拟基方法（`virtual void Next() =
0;`) 是提取器不会作为节点发出的声明，因此无法覆盖纯抽象接口
桥接（仅具有真实方法节点的基础 - 内联默认或非纯虚拟）；加上C
回调结构扇出。依赖于确定性验证（无 A/B）：跨文件调用计数 + 精确
覆盖抽查是结论性的。
- **边境通行证（2026-05-23）-易于处理的部分关闭，噪音/困难的部分故意离开。** 在主要部分之后
扫描，扫描记录的边界并按精度/值进行分类。 **完成：** React Router 对象
数据路由器（字面值 `createBrowserRouter([{path, element}])`）； Next.js 路由误报（配置文件 +
`nextjs-pages/` 子字符串→需要真实的页面分机+路径段匹配；防弹4→0); Flask-RESTful
`add_resource`→资源类（红线6→**77**）；烧瓶元组 `methods=(…)`；烧瓶检测范围扩大到
子目录/应用程序工厂入口点（flask-realworld 0→**19**）；大猩猩/多路复用器确认已覆盖（任何接收器
HandleFunc) + 测试。 **左（有基本原理，而不是双关语）：** C 回调结构调度 (`cmd->proc()` →
422 路场扇出 = 噪声）；元编程查找器（ActiveRecord/Eloquent/Spring-Data-JPA/EF — 动态
命名，无静态目标）；反应式运行时（Vue Proxy / Compose 重组 - 深层内部结构，无
setState 风格的门）； Akka actor 消息调度（无类型）；纯匿名内联闭包（def-use
边界——没有指定目标）； React 惰性数据路由器（变量路径+惰性导入）； C++ 纯虚基
方法（提取无体 decls 会冒重复 decl/def 节点以获得适度增益的风险）。强制这些会增加
噪音，违反了“部分覆盖比没有覆盖更糟糕”的规定。
- **难度梯度是真实的：**named-ref 调度（解析器）很便宜；匿名的
回调调度（合成器）中等； **匿名箭头处理程序是困难的
剩余间隙**（没有身份→需要合成器通过主体链接，尚未构建）。
- **提取变化的影响范围很大。** 第 3 阶段命名内联回调
提取位于 *共享* `tree_sitter.rs` walker 中 - 重新检查 **节点计数
任何提取更改后的几种语言**（它在 exalidraw 上保持在 +3，因为
匿名箭头被跳过）。
- **合成器精度保护：** 注册商名称唯一性、仅命名处理程序和
事件**扇出上限**（跳过 `error`/`change` 等通用事件）。接收器型
匹配（通过 `type_of` 边缘）是计划中的精度升级 - 已推迟。
- **内置快捷方式**（回调合成器）：通过*文件*+字段对注册器/调度器进行配对
（类代理），正则表达式 arg-recovery（仅命名引用），`provenance:'heuristic'` +
`metadata.synthesizedBy`（枚举没有 `'callback-synthesis'`）。请参阅设计文档。
- **合成器仅在 `resolveAndPersistBatched` 中运行**（完整索引）- 连接到
`resolveAndPersist` 用于发货前增量同步。
- **`trace` 中的符号歧义：** 通用名称（`render`、`execute_sql`）与许多匹配
节点；跟踪在其中进行选择，并且可能从错误的开始。从具体追踪
方法，而不是类名。

---

## 8. 完成的定义（整个任务）

对于每种语言 × 框架：端到端的规范流程 `trace`，代理可以
至少在一些存在胶水、无节点的运行中使用 Read 0 回答流程问题
爆炸，没有回归——记录在矩阵（§6）中，并带有验证回购+数字。
然后发货准备：按机制进行测试、变更日志、增量传输、提交。


## 2026-11-02：已有 TS/React 大文件覆盖恢复

本次是既有机制的大文件回归修复，不新增语言或框架。控制仓库为 Excalidraw 8bcdca4526867433f99fdfcb55a063668db0c9dc（738 文件，中型），不能与历史 643 文件样本混算。旧索引因体积阈值退回文本扫描，App.tsx 出现 if/return 伪成员，triggerRender 被误标 property。恢复原生 AST 与多行字段分类后，Scene.mutateElement → triggerUpdate → [callback] App.triggerRender → [react-render] App.render → [jsx-render] StaticCanvas → renderStaticScene 在同一次 explore 第一条 Flow 中完整展示，三个边界均带接线位置；返回源码包含 triggerRender 函数体。两次完整新索引节点均为 11553，无节点增长。

维护者明确允许此任务改用当前 Codex 模型，实测两分支均为 gpt-6.1-sol/high、各 n=2，MCP 预热且实际使用。基线耗时 141–179s、总工具 32–37；新版本 115–123s、总工具 25–30；非 MCP 仓库源码 Read/Grep 均为 0。仍未达到 3–5 次工具目标，不能将此条标成所有规模矩阵完成。详细原始摘要、测试与两项已在基线复现的 Java 失败见 [大文件 Flow 报告](../benchmarks/large-file-flow-2026-11-02.md)。


<a id="nix-static-coverage"></a>

### Nix 静态覆盖收口（2026-11-04）

支持 `.nix` 的命名绑定、柯里化函数、属性集与继承属性抽取、静态调用，
以及项目内相对 `import` / `builtins.import` / `callPackage` / `callPackages`
路径解析（目录使用 `default.nix`）。增量同步保留受影响导入者的接线，
删除目标或导入者后的结果与重建保持一致。动态路径、动态属性、部分应用、
内建函数求值与运行时模块组合不在流程覆盖承诺内；不为这些边界猜测合成边。

来源固定为上游 `15483713763f344ff30fb7df92bcf091c7bd9758`。
本地实现与测试见提交 `105781b9`、`18844f52`、`5827a36b`、`10182c55`、
`b9371b0d`、`7b1580e4`；这些提交记录原生 grammar、抽取、解析和增量同步的
聚焦验证，包含真实 parser 与 SQLite 检查。此文档任务未重新运行 Cargo。

[九案例矩阵](../benchmarks/upstream-matrices/nix.md)、
[固定输入](../benchmarks/upstream-matrices/nix.json)与
[历史静态 Flow 证据](../benchmarks/upstream-matrices/nix-evidence.json)
在 `e65171a3` 中归档，覆盖 flake-utils、Home Manager、Nixpkgs 各三个问题。
证据的实现版本为 `51057670b77b35cb25a9cef587deff82e3828315`；
各案例保留有序 Flow、节点、相邻边、源码位置及哈希。
这些记录只支持所选静态路径，不证明完整 Nix 求值或代理性能。

按用户 2026-11-03 修订，批次 70–78 的九项外部验收均按“用户取消验收”收尾：
未执行对应的真实仓库回放、重复 Agent A/B、指定模型或平台实验，
不声明 Read/Grep、延迟、工具调用预算或控制仓库回归验收通过。
上述历史静态记录不替代已取消实验的结果；矩阵说明中的外部运行步骤仅为历史复现信息。
本次仅核对文档与实现、固定来源及九案例记录的一致性，并执行 `git diff --check`。


<a id="terraform-static-coverage"></a>

### Terraform / OpenTofu 静态覆盖收口（2026-11-04）

支持 `.tf`、`.tofu` 声明抽取、目录作用域内的静态引用、本地模块输入与输出、
父作用域 provider alias，以及无歧义的 Cloud Posse component remote-state 输出。
增量同步会重建受影响的作用域引用。动态 source、远程模块下载、运行时求值与
存在歧义的目标不属于此支持声明。

来源为固定上游 `15483713763f344ff30fb7df92bcf091c7bd9758`；原生 grammar
与声明抽取、引用解析、provider 接线及增量实现分别见提交
`949c987c`、`cf07af9e`、`48ed8f26`、`25be52dc`、`66f9b8ae`、`4cf10fc0`。
本次为文档核对，仅执行 `git diff --check`，没有重新运行 Cargo 测试。

依赖批次 88–96 的九项真实仓库流程验收已由用户于 2026-11-03 取消，
不再作为支持声明门槛。[取消验收清单](../benchmarks/upstream-matrices/terraform.json)
在 `96133f22` 归档，案例数组为空，并非九案例通过报告。
真实仓库回放、重复 A/B、性能与 VM/Docker 验证均未执行；不声明这些外部验证通过。


<a id="solidity-static-coverage"></a>

### Solidity 静态覆盖收口（2026-11-04）

支持 `.sol` 中的 contract、interface、library、struct、event、modifier、
函数、构造函数以及 fallback/receive 声明抽取；记录调用、modifier 调用、
构造与继承引用。仅在目标唯一且位于已索引文件内时解析静态引用，支持普通、
命名别名与 namespace 导入，以及沿显式基类解析裸成员调用。
局部成员优先；歧义继承与不可访问的 private/external 基类函数不接边。
增量同步在文件修改、新增和删除后保留跨文件引用，与干净重建对齐。

此声明不包含完整 Solidity 编译器语义：不解析运行时接收者派发、歧义重载、
完整多继承线性化或项目 remapping 配置，不下载外部包；不承诺枚举、状态变量、
自定义 error、emit/revert、using-for 扩展或 modifier 占位符的执行顺序覆盖。
抽取到成员调用文本并不意味着能够解析其运行时目标。

来源为只读上游 `15483713763f344ff30fb7df92bcf091c7bd9758` 的
`src/extraction/languages/solidity.ts`；本地原生 grammar 固定于
`b239a95f94cfcc6e7b3e961bc73a28d55e214f02`，详见
[grammar 来源与可行性记录](upstream-grammars/solidity.md)。
接线、声明、调用、导入解析、继承解析与增量修复分别见提交
`085bc498`、`4599d2ae`、`95733643`、`48dc15d7`、`4cb58989`、`06e2e2d9`；
本地回归用例位于 `tests/upstream_099_test.rs` 至 `tests/upstream_104_test.rs`。
本次只核对文档与实现、测试断言的一致性，并执行 `git diff --check`，没有重跑 Cargo。

依赖批次 106–114（任务 45–53）的九项真实仓库流程验收适用用户
2026-11-03 的取消修订，不再作为支持声明门槛。
[取消验收清单](../benchmarks/upstream-matrices/solidity.json) 在 `c033556e` 归档，
`cases` 为空，`external_experiments_executed` 为 false；没有九案例 summary
或原始运行日志，也不将其视作通过报告。真实仓库回放、重复 A/B、性能与
VM/Docker 验证均未执行，不声明外部验收通过。

<a id="erlang-static-coverage"></a>

### Erlang 静态覆盖收口（2026-11-04）

支持 `.erl`、`.hrl`、`.escript`，抽取模块 namespace、按名称/参数个数区分的函数、
合并的连续函数子句、export/export_all 可见性与宏常量。记录静态本地和远程调用、
函数引用、宏引用、模块导入、header 导入与 behaviour 声明；按唯一目标解析索引内
模块、显式 import 与相对 header，不回退到模糊同名匹配。

变量模块调用的 behaviour 桥接属于启发式候选关系，不证明运行时接收者绑定：
仅在索引内相同名称/参数个数对应唯一 callback 契约时，连接显式声明该 behaviour
且导出匹配函数的实现。歧义契约、重复模块、私有函数、参数个数不匹配不接边，
超过 24 个候选目标时保持沉默；边带 heuristic 来源、合成器名称和调用位置。
增量同步覆盖声明/参数个数变化、目标或契约删除、模块重建与无变化同步，
并与干净索引比较节点和边。

不承诺完整预处理、条件编译、宏展开、record/type/spec 声明索引、OTP 应用路径
查找、外部依赖加载、通用 OTP 消息/进程派发或动态 apply/函数变量求值。
函数引用不等同于执行调用，behaviour 候选边不等同于精确的数据流解析。

来源为只读上游 `15483713763f344ff30fb7df92bcf091c7bd9758` 的
`src/extraction/languages/erlang.ts`；原生 ELP grammar 0.19 固定于
`836aa2b6c3af2c7cef3f84049b0ed6d44485a870`，详见
[grammar 来源与可行性记录](upstream-grammars/erlang.md)。
接线、声明、调用、跨文件解析、behaviour 桥接与增量修复分别见提交
`cfd7860a`、`7aadf8f8`、`9ab87f45`、`5baa1aeb`、`aeb73ab2`、`15126c22`；
本地回归用例为 `tests/upstream_117_test.rs` 至 `tests/upstream_122_test.rs`。
本次只核对文档、实现与测试断言的一致性，并执行 `git diff --check`，没有重跑 Cargo。

依赖批次 124–132（任务 63–71）的九项真实仓库流程验收适用用户
2026-11-03 的取消修订，不再作为支持声明门槛。
[取消验收清单](../benchmarks/upstream-matrices/erlang.json) 在 `c642ff07` 归档，
`cases` 为空，`external_experiments_executed` 为 false；没有九案例 summary
或原始运行日志，也不将其视作通过报告。真实仓库回放、重复 A/B、性能与
VM/Docker 验证均未执行，不声明外部验收通过。

<a id="vbnet-static-coverage"></a>

### VB.NET 静态覆盖收口（2026-11-04）

支持 `.vb`，抽取 namespace、class、module、Function/Sub 方法、property 与 event
声明；event 作为 field 保存，property 的 Get/Set 体内普通调用归属于该 property。
记录 Imports、Inherits、Implements 与静态调用；跨文件按大小写不敏感的完整名称、
namespace Imports、Imports 别名、Global 名称和当前作用域匹配唯一索引目标，
不把 namespace 当文件路径，也不以模糊同名匹配补边。
增量同步覆盖声明变化、目标删除重建、Imports 变化、调用方删除和无变化同步，
并与干净索引比较节点和边。

不承诺完整 VB 编译器语义、外部程序集加载、泛型/重载类型推断、动态接收者或
运行时事件派发；Get/Set 普通调用不是事件合成边。当前不单独抽取 interface、
structure、enum 和局部变量声明。grammar 有列首 Return/label 与 XML literal
方面的已知限制，末尾缺换行会在解析前补齐，原有源码偏移保持不变。

来源为只读上游 `15483713763f344ff30fb7df92bcf091c7bd9758` 的
`src/extraction/languages/vbnet.ts`；原生 grammar 固定于
`538b7087bf80e86004531b392fe1186379c0a2b5`，附该上游版本的 grammar patch，
详见 [grammar 来源与可行性记录](upstream-grammars/vbnet.md)。
接线、声明、引用、跨文件解析、访问器静态流程与增量修复分别见提交
`b8a80314`、`3bafb477`、`97b8b699`、`8ff6599a`、`9f3c4a22`、`95ec511c`；
本地回归用例为 `tests/upstream_135_test.rs` 至 `tests/upstream_140_test.rs`。
本次只核对文档、实现与测试断言的一致性，并执行 `git diff --check`，没有重跑 Cargo。

依赖批次 142–150（任务 81–89）的九项真实仓库流程验收适用用户
2026-11-03 的取消修订，不再作为支持声明门槛。
[取消验收清单](../benchmarks/upstream-matrices/vbnet.json) 在 `e240ddd0` 归档，
`cases` 为空，`external_experiments_executed` 为 false；没有九案例 summary
或原始运行日志，也不将其视作通过报告。真实仓库回放、重复 A/B、性能与
VM/Docker 验证均未执行，不声明外部验收通过。

<a id="cobol-static-coverage"></a>

### COBOL 静态覆盖收口（2026-11-04）

支持 `.cbl`、`.cob`、`.cpy`（扩展名大小写不敏感），使用原生 AST 抽取
PROGRAM-ID 为 module、section 与 paragraph 为 function，并重建平铺过程头的
范围和包含关系；支持固定格式及按声明检测后预处理的自由格式、copybook 片段。
记录静态 PERFORM（THRU 记录两个端点，不展开区间）、字符串 literal CALL 和 COPY。
CALL 只匹配唯一的索引内同名 program；PERFORM 限于当前文件的当前 program，
名称按 ASCII 大小写不敏感匹配。COPY 优先同目录，显式相对路径按路径匹配，
否则按索引文件名或 stem 匹配唯一目标；不是编译器 copybook 搜索路径实现。
增量同步覆盖声明变化、目标与 copybook 删除重建、重名歧义增加或消除，并与
干净索引比较节点和边，保留非 COBOL 控制文件的图内容。

不承诺完整编译器语义、COPY 文本展开或 REPLACING 求值、数据项声明抽取、
变量 CALL 目标推断、运行时框架派发及外部程序加载；歧义目标保持未解析，
不以同名猜测补边，也没有新增动态派发合成器。PERFORM THRU 的端点边不代表
区间内所有 paragraph 的执行顺序。

来源为只读上游 `15483713763f344ff30fb7df92bcf091c7bd9758` 的
`src/extraction/languages/cobol.ts`；原生 grammar 固定于
`e99dbdc3d800d5fa2796476efd60af91f6b43d93` 并应用该上游的 patch，
详见 [grammar 来源与可行性记录](upstream-grammars/cobol.md)。
接线、声明、引用、跨文件解析、静态流程边界与增量修复分别见提交
`5220c69d`、`a571f026`、`8a42105b`、`98aa0008`、`83db25c3`、`ae59653d`；
本地回归用例为 `tests/upstream_153_test.rs` 至 `tests/upstream_158_test.rs`。
本次只核对文档、实现与测试断言的一致性，并执行 `git diff --check`，没有重跑 Cargo。

依赖批次 160–168（任务 99–107）的九项真实仓库流程验收适用用户
2026-11-03 的取消修订，不再作为支持声明门槛。
[取消验收清单](../benchmarks/upstream-matrices/cobol.json) 在 `50bce593` 归档，
`cases` 为空，`external_experiments_executed` 为 false；没有九案例 summary
或原始运行日志，也不将其视作通过报告。真实仓库回放、重复 A/B、性能与
VM/Docker 验证均未执行，不声明外部验收通过。


<a id="cfml-static-coverage"></a>

### CFML 静态覆盖收口（2026-11-04）

支持 `.cfm`、`.cfc`、`.cfml`（扩展名大小写不敏感），使用原生 tag AST，
对裸 script 和嵌入的 cfscript 重新解析并保留原始源码位置。抽取 component 为
class、组件内函数为 method、顶层函数为 function，记录访问可见性和包含关系。
记录静态 new/createObject 组件构造、extends、相对 template include 和调用；
组件路径优先当前目录，再匹配索引根目录下完整路径，名称按大小写不敏感匹配。
普通调用限于同文件唯一函数或方法，支持 this/variables 限定；直接链在静态
组件构造后的方法调用按已解析组件限定，不回退到其他组件的同名方法。
增量同步覆盖声明变化、组件与模板删除重建、本地组件遮蔽根目录组件，
并与干净索引比较节点和边。

不承诺完整 CFML 编译器语义、运行时应用/服务器路径映射、变量接收者推断、
动态路径求值、框架运行时派发或外部依赖加载；缺失及歧义目标保持未解析。
当前不单独抽取 property、参数和局部变量声明，也不分析 cfquery 内嵌 SQL，
没有新增动态派发合成器。原生 grammar 与上游 WASM 的确切版本等价性未经证明。

来源为只读上游 `15483713763f344ff30fb7df92bcf091c7bd9758` 的
`src/extraction/cfml-extractor.ts`、`src/extraction/languages/cfscript.ts` 和
`src/extraction/languages/cfquery.ts`；原生 grammar 固定于
`24051c091c9c1a69b36914533aee7aa3286d5526`（0.26.44），详见
[grammar 来源与可行性记录](upstream-grammars/cfml.md)。
接线、声明、引用、跨文件解析、构造链调用与增量修复分别见提交
`95821ffe`、`496c7e84`、`93905c5b`、`c82a9531`、`ea94bafe`、`de88414a`；
本地回归用例为 `tests/upstream_171_test.rs` 至 `tests/upstream_176_test.rs`。
本次只核对文档、实现与测试断言的一致性，并执行 `git diff --check`，没有重跑 Cargo。

依赖批次 178–186（任务 117–125）的九项真实仓库流程验收适用用户
2026-11-03 的取消修订，不再作为支持声明门槛。
[取消验收清单](../benchmarks/upstream-matrices/cfml.json) 在 `59295eae` 归档，
`cases` 为空，`external_experiments_executed` 为 false；没有九案例 summary
或原始运行日志，也不将其视作通过报告。真实仓库回放、重复 A/B、性能与
VM/Docker 验证均未执行，不声明外部验收通过。


<a id="arkts-static-coverage"></a>

### ArkTS / ArkUI 静态覆盖收口（2026-11-04）

支持 `.ets`，使用原生 AST 抽取 struct、method、function（含 Builder）与 import，
记录 ArkUI 组件调用、链式属性及直接传入 onX 属性的 this 方法引用。
相对导入支持显式扩展名、目录 index、命名别名及 namespace；未导入名称仅在
当前文件解析，缺失或外部导入不回退到其他文件的同名声明。
对 Component struct 自身响应式字段的直接赋值、增减及受支持数组变更，
合成方法 → build 调用边，再接已有的 build → 已导入子组件静态边；
合成边标记 heuristic、synthesizedBy=arkui-state 和 registeredAt。
增量同步覆盖导入切换、组件与调用方删除重建，并与干净索引比较节点和边。

不承诺完整 ArkTS 类型系统、外部 HarmonyOS API 加载、动态接收者推断或
完整 ArkUI 运行时语义。字段、局部变量、class、interface、enum 和 type alias
当前不单独抽取；响应式字段由合成器读取 AST，不因此新增字段节点。
响应式分析跳过其他接收者、嵌套函数、读取、比较和歧义 build；不追踪
字段别名、任意深层对象写入或跨组件运行时状态传播。

来源为只读上游 `15483713763f344ff30fb7df92bcf091c7bd9758` 的
`src/extraction/languages/arkts.ts`；原生 grammar 为 0.2.0，固定于
`56f7fc288715befe92c734d603f9e6bc3c65d2a8`，详见
[grammar 来源与可行性记录](upstream-grammars/arkts.md)。
接线、声明、引用、跨文件解析、响应式渲染与增量修复分别见提交
`7cad7a92`、`e7f0bfc8`、`794d9c0e`、`e201606e`、`47919f99`、`ef705feb`；
本地回归用例为 `tests/upstream_189_test.rs` 至 `tests/upstream_194_test.rs`。
增量修复提交记录本地 3 项通过，导入与渲染周边 11 项通过、0 ignored；
任务 132 的延后本地回归已于 2026-11-04 完成：此前两项 Java 字段注入失败
在最新工作区仍可复现（框架套件 16 passed / 2 failed，退出 101），与原先移除
ArkTS 接线后的基线证据一致。本轮修复保留 Java 字段类型签名、归一化
`this.field` 接收者，并避免把兼容抽取的同义类误当嵌套类；方法匹配优先使用
带签名的原生声明，测试也明确选择原生调用方，不再依赖同义节点的返回顺序。
原有边断言保留，并新增控制器 → bean → 接口 → 实现的连续三跳断言。

本轮执行 `cargo fmt --all`（退出 0）及以下本地检查（退出 0）：

```sh
cargo test -p rustcodegraph --test frameworks_integration_test --test extraction_test --test resolution_test --test upstream_189_test --test upstream_190_test --test upstream_191_test --test upstream_192_test --test upstream_193_test --test upstream_194_test -- --nocapture
```

合计 580 passed / 0 failed / 78 既有 ignored，其中框架套件 18 项、任务 132
聚焦 5 项均通过且无 ignored，ArkTS 接线至增量套件共 24 项通过。
验证使用动态槽位 1，`CARGO_TARGET_DIR` 与 `CARGO_BUILD_BUILD_DIR` 均为
仓库根目录下的 `target/rust-slot-1`；结束后释放占用锁并保留缓存。
任务 132 的本地回归阻塞已解除；此结果不代表未执行的外部验收通过。

依赖批次 196–204（任务 135–143）的九项真实仓库流程验收适用用户
2026-11-03 的取消修订，不再作为支持声明门槛。
[取消验收清单](../benchmarks/upstream-matrices/arkts.json) 在 `073575bf` 归档，
`cases` 为空，`external_experiments_executed` 为 false；没有九案例 summary
或原始运行日志，也不将其视作通过报告。真实仓库回放、重复 A/B、性能与
VM/Docker 验证均未执行，不声明外部验收通过。


<a id="cuda-static-coverage"></a>
### CUDA 静态覆盖收口（2026-11-04）

支持 `.cu`、`.cuh`（扩展名大小写不敏感），复用现有 C++ grammar，
节点语言仍为 `cpp`，没有新增第三方解析器副本。
预处理屏蔽受支持的 CUDA 修饰符、launch bounds 和完整的 kernel launch
配置，保留字节位置与换行，再由 C++ AST 抽取声明与静态调用。
本地测试覆盖 host → kernel → device helper、UTF-8/CRLF 坐标、
注释与字符串保护，以及不完整 launch 后续代码恢复。
本地 `.cuh` 与 `.h` include 可解析到索引内目标；头文件变更及逐个删除
头文件、调用方后的增量图与全量重建比较，并检查无悬空边。
缺失头文件和外部 CUDA SDK 不制造目标节点。

此覆盖不等同于 CUDA 编译器或 GPU 运行时分析：不推断线程、stream、
异步调度、内存同步或动态调用；不加载外部 SDK。
Launch 配置的扫描有界，复杂或不完整语法不保证识别；常量和 launch
配置表达式的运行语义不在覆盖范围内。

计划来源固定为只读上游 `15483713763f344ff30fb7df92bcf091c7bd9758`；
实际 Rust 抽取与增量实现见 `9ae81776`、`e418f20c`，
本地用例为 `tests/upstream_206_test.rs` 和 `tests/upstream_207_test.rs`。
已核对增量实现的原始本地日志 `/tmp/rcg207-green.log`：
两套用例分别 4 项与 2 项通过，均 0 failed、0 ignored；
周边 include 解析日志 `/tmp/rcg207-neighbor.log` 为 12 passed、1 ignored。
这些是前置实现的既有证据，本次仅审查文档与代码、测试断言的一致性，
执行文档清单检查和 `git diff --check`，未重跑 Cargo，也不宣称全套回归通过。

依赖批次 209–217（任务 148–156）的九项真实仓库流程验收适用用户
2026-11-03 的取消修订，不再作为支持声明门槛。
[取消验收清单](../benchmarks/upstream-matrices/cuda.json) 已由 `fdfcf466` 归档，
其 `cases` 为空、`external_experiments_executed` 为 false，状态为“用户取消验收”。
九项均未执行，没有案例 summary 或原始实验日志，不能视作验收通过；
真实仓库回放、重复 A/B、性能指标及 VM/Docker 验证均未执行。


<a id="metal-static-coverage"></a>
### Metal 静态覆盖收口（2026-11-04）

支持 `.metal`（扩展名大小写不敏感），复用现有 C++ grammar，节点语言为
`cpp`，不新增解析器副本。预处理屏蔽受支持的尾置 `[[attribute]]`，保留
UTF-8 字节位置与换行；抽取声明和静态函数调用。局部测试覆盖结构与字段、
kernel/vertex/fragment 函数到 helper 的调用，以及注释、字符串、原始字符串
和 lambda 的保护；普通 `.cpp` 不应用 Metal 属性预处理。

这不是完整 Metal 编译器或 GPU 运行时分析。不承诺完整属性语法、复杂嵌套
属性参数、GPU 调度、资源绑定的运行语义或外部 SDK 解析；未知外部调用
不制造目标声明。本地头文件引用可连接到索引内目标；本地用例验证头文件
替换以及逐个删除头文件、Metal 调用方后，增量图与全量重建一致且无悬空边。
缺失头文件和外部 `metal_stdlib` 不制造目标边。

来源为只读上游 `15483713763f344ff30fb7df92bcf091c7bd9758`；Rust 实现为
`dca1ebba`（抽取）和 `525324d6`（增量接线）。已核对前置任务 159 原会话
`01a1a569-0e02-7332-8b82-a307891eb044` 的实际命令、退出码与
`/tmp/rcg-159-validation.log`：修复前行为验证退出 101；修复后先运行
`cargo fmt --all`，再运行
`cargo test -p rustcodegraph --test upstream_220_test --test upstream_219_test --test upstream_207_test -- --nocapture`，
退出 0，Metal 增量 1 项、Metal 抽取 4 项及 CUDA 周边 2 项全部通过，
合计 7 passed、0 failed、0 ignored。实际槽位为 1，
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1`，
原会话已释放锁。上述实现与测试相对 `525324d6` 无后续差异。

[前置清单](../benchmarks/upstream-matrices/metal.json) 已补充此有效通过证据；
先前源码对应关系未确认的旧二进制失败不能代表修复后的结果。
本次进行原会话、日志、提交与文档的一致性核对及 `git diff --check`，
没有重跑 Cargo，也不宣称全套回归通过。

依赖批次 222–230（任务 161–169）的九项外部流程验收已按用户
2026-11-03 修订取消，不再作为本次文档收口门槛。取消清单提交为 `ba74eb7b`，
`cases=[]`、`external_experiments_executed=false`，状态为“用户取消验收”。
九项均未执行，没有案例 summary 或原始实验日志；真实仓库回放、Agent A/B、
性能指标及 VM/Docker 实验均未执行，不声明这些验收通过。


<a id="redux-rtk-static-coverage"></a>
### Redux / RTK 静态覆盖收口（2026-11-04）

沿用 TypeScript/TSX 与 JavaScript/JSX 抽取，不新增语言或扩展名。
Redux Toolkit 支持从 `@reduxjs/toolkit` 导入（含别名）的
`createAsyncThunk` 注册：连接 callback 中直接 `dispatch(thunk())` 到唯一的
本地或导入注册目标，支持多级 thunk 链。要求第二个参数解构出 `dispatch`；
遮蔽绑定、延迟嵌套 callback、未注册目标不合成该调用，单个来源最多连接
24 个目标。这不是任意 redux-thunk、中间件、reducer 或 store 运行时分析。

RTK Query 支持从 `@reduxjs/toolkit/query/react` 导入的 `createApi`，
在同模块字面量 endpoints 中识别 builder 的 query/mutation 与唯一内联
`query`/`queryFn` callback，再从 API 解构出的 query、lazy-query、mutation
hook（含本地别名）连接到该 callback。既有普通调用边可继续连接导入的
调用方和 callback 内 helper；本地 explore 测试验证完整
`View → useGetItemsQuery → getItems → fetchItems` 路径及 wiring 标注。
不承诺 `injectEndpoints`、跨模块 API 注册、动态键、运行时缓存/网络调度
或所有间接 dispatch 形式；检测到的重赋值、配置展开、重复注册与同名歧义
保持无合成边。两类合成边均标为 heuristic，并带 `registeredAt`。

来源固定为只读上游 `15483713763f344ff30fb7df92bcf091c7bd9758`，
已核对本地上游 HEAD。Rust 实现提交为 `32464a89`（thunk）与
`fb4a7cde`（RTK Query）；当前实现与这两个提交后的版本一致。
已核对既有 `/tmp/rcg233-regression-final.log`：
`upstream_232_test` 5 项、`upstream_233_test` 6 项、object-literal methods
6 项、explore 预算 4 项及 flow 3 项通过，合计 24 passed、0 failed、0 ignored。
这包括跨文件别名/同名隔离、缺失与动态注册负例、重复索引稳定性和删除接线。
`fb4a7cde` 提交说明记录初始缺失 hook 的失败探针、最终通过和共享 Cargo
槽位 1；本次仅复核这些既有证据、源码及断言，运行文档清单检查与
`git diff --check`，没有重跑 Cargo，也不声明全套回归通过。

依赖批次 235–243 的九项外部验收适用用户 2026-11-03 的取消修订，
按“用户取消验收”收尾，不再作为本次文档门槛。
[取消验收清单](../benchmarks/upstream-matrices/redux.json) 由 `1fa51138`
归档，`cases=[]`、`external_experiments_executed=false`；上游仅有两个
未冻结目标版本的候选问题，不构成九案例覆盖矩阵。
九项均未执行，没有案例 summary 或原始实验日志；真实仓库回放、重复 A/B、
指定模型、性能指标与 VM/Docker 验证均未执行，不声称外部验收通过。


<a id="vue-store-static-coverage"></a>
### Vue 状态仓库静态覆盖收口（2026-11-04）

沿用 TypeScript/TSX 与 JavaScript/JSX 提取，不新增语言或扩展名；本次
注册识别与合成器限定为这四种语言，不据此承诺 `.vue` SFC 脚本内的 store
派发覆盖。来源固定为只读上游 `15483713763f344ff30fb7df92bcf091c7bd9758`，
已核对本地上游 HEAD，以及提取、Pinia 与 Vuex 三份上游测试的来源清单。
Rust 实现提交为 `110d7d46`（注册提取）与 `382217e6`（端到端派发）。

提取器识别从 `pinia` / `vuex` 导入（含别名）的 `defineStore` / `createStore`
模块级字面量配置，及 `namespaced: true` 的 default-export Vuex 模块配置。
从内联或同模块稳定对象中的 `actions`、`mutations`、`getters` 提取静态名称
的方法、箭头函数和函数表达式，保留函数体调用；提取 getter 不代表覆盖
getter 属性读取或状态依赖。

Pinia 支持 Options store：实例绑定直接调用可见的本地或导入 store factory，
再将 `instance.action()` 连接到该注册的唯一 action。Vuex 支持可见的
`createStore` 注册及静态 `modules`（含导入的 default-export namespaced
模块），将字符串字面量 `store.dispatch` / `store.commit` 连接到 action /
mutation，并支持 action 参数解构出的裸 `dispatch` / `commit` 的模块内调用。
已有普通调用边继续连接 handler 内 helper。本地 explore 断言覆盖
`Entry → login → finish` 与 `Entry → login → SAVE → finish` 完整路径。
合成边标为 heuristic，带 `synthesizedBy` 和 `registeredAt`，保留接线位置。

不承诺 Pinia setup store、`this.action()`、解构 action、`mapActions` /
`mapMutations`、Vuex `new Store`、动态模块注册、动态键、任意 re-export 链、
Vuex root-option 跨命名空间派发或 Vue Proxy 状态到渲染链。
检测到的绑定遮蔽、重赋值、未注册目标与多个候选目标保持无合成边，不按
全局同名 action 猜测目标；静态对象与模块遍历有深度限制。

本次核对既有 `/tmp/rcg246-final.log`：`upstream_245_test` 与
`upstream_246_test` 各 4 项通过，覆盖注册正负例、导入别名、同名 store
隔离、动态键/遮蔽/重赋值负例、重复索引稳定性和 sync 移除接线。
同一日志另记录 Redux/RTK 控制测试 11 项、explore 预算/自适应/Flow
31 项、closure collection 1 项通过；合计 51 passed、0 failed、0 ignored。
当前提取模块、派发模块及行为测试与对应实现提交一致。本任务只复核既有
日志、源码与断言，执行文档清单检查及 `git diff --check`，不重跑 Cargo，
不声明全套回归通过。

已逐项复核依赖批次 248–256（任务 17–25）的会话收尾记录：九项均按用户
2026-11-03 修订以“用户取消验收”收尾，不再作为本次文档门槛。
[取消验收清单](../benchmarks/upstream-matrices/vue-store.json) 由 `8fae8c94`
归档，`cases=[]`、`external_experiments_executed=false`。
清单仅列上游合成 fixture，不构成固定版本的小中大九案例真实仓库矩阵。
九项均未执行，没有案例 summary 或原始实验日志；真实仓库回放、重复 A/B、
指定模型、性能指标及 VM/Docker 验证均未执行，不声明外部验收通过。


<a id="celery-static-coverage"></a>
### Celery 静态覆盖收口（2026-11-05）

沿用 Python `.py` 提取，不新增语言或扩展名。注册通过 AST 识别
`@task`、`@shared_task` 及以 `.task` / `.shared_task` 结尾的点分装饰器，
支持带参数装饰器；这属于语法识别，不验证装饰器来自真实 Celery app。
仅将唯一的模块级注册函数作为派发目标，不新增任务节点。

派发将函数体内的 `task.delay()` / `task.apply_async()` 连接到同文件注册，
或顶层显式 import 指向的注册任务；支持任务别名、模块别名及相对导入。
既有普通调用边继续连接任务体内 helper。本地 explore 断言覆盖
`Entry → work → finish`，并检查 heuristic、`celery-dispatch` 与
`registeredAt` 接线标注。每文件最多连接 80 个不同来源/目标对。

不承诺字符串任务名、`send_task`、canvas group/chain/chord、动态属性、
运行时任务注册、任意 re-export、broker 投递或 worker 调度。
未加别名的多段模块 import、嵌套 import、绑定遮蔽、检测到的重赋值、
冲突定义及多个候选保持无合成边；不按全项目同名任务猜测目标。

来源固定为只读上游 `15483713763f344ff30fb7df92bcf091c7bd9758`，
本次已核对上游 HEAD。Rust 注册与派发提交分别为 `ebb3a61b`、`22097c15`，
当前实现与注册/派发测试相对派发提交无差异。
复核既有 `/tmp/rcg259-final.log`：注册 3 项、派发 4 项及 function-reference
回归 18 项通过，合计 25 passed、0 failed、0 ignored；日志使用
`target/rust-slot-1`。断言包括导入别名和同名任务隔离、相对多行导入、
遮蔽/动态调用负例、重复索引节点稳定性，以及移除装饰器后 sync 删除接线。
本次只核对既有日志、源码与断言，执行文档清单检查和 `git diff --check`，
没有运行 Cargo，不声明全套回归通过。

依赖批次 261–269（任务 30–38）的九项外部验收按用户 2026-11-03
修订以“用户取消验收”收口，不再作为支持范围文档的完成门槛。
[取消验收清单](../benchmarks/upstream-matrices/celery.json) 已由 `3a92a175`
归档，`cases=[]`、`external_experiments_executed=false`。
上游合成 fixture 不构成固定版本的小中大九案例真实仓库矩阵。
九项均未执行，没有案例 summary 或原始实验日志；真实仓库回放、重复 A/B、
指定模型、性能指标及 VM/Docker 验证均未执行，不声明外部验收通过。


<a id="sidekiq-static-coverage"></a>
### Sidekiq 静态覆盖收口（2026-11-05）

沿用 Ruby `.rb` 提取，不新增语言或扩展名。AST 注册证据要求类体直接
`include Sidekiq::Worker` 或 `include Sidekiq::Job`（也接受前导 `::`），
并有唯一的直接实例 `perform` 方法；不把字符串、注释、嵌套类的方法、
单例方法或方法体内 include 当作该类的注册证据，不新增 worker 节点。
这是语法识别，不验证 Sidekiq 常量来自实际安装的 gem。

将函数或方法体内常量接收器的 `perform_async`、`perform_in`、`perform_at`
连接到唯一注册 worker 的 `perform`，已有普通调用边继续连接其静态 helper。
Rails 风格的同一个 `app/` 树共享注册；其他 Ruby 文件按所在目录隔离。
限定名称精确匹配，短名称要求唯一；类或 module 内要求以 `::` 开头的绝对
接收器，未证明的相对词法名称保持无合成边。每文件最多连接 80 个不同
来源/目标对，边标记 heuristic、`sidekiq-dispatch` 和调用位置 `registeredAt`。

不承诺继承或间接 include 的注册、ActiveJob `perform_later`、动态接收器、
`const_get`、常量别名或重绑定、跨应用注册、运行时队列投递与调度执行；
同名歧义保持无边。调用时间参数不用于推断实际执行顺序。

来源固定为只读上游 `15483713763f344ff30fb7df92bcf091c7bd9758`，
本次已核对其 HEAD。注册与派发提交分别为 `75f76202`、`bbea5e3f`，
当前注册/派发实现和两份测试相对派发提交无差异。
复核既有 `/tmp/rcg272-final.log`：`upstream_271_test` 注册 4 项、
`upstream_272_test` 派发 4 项及 `upstream_259_test` Celery 回归 4 项通过，
合计 12 passed、0 failed、0 ignored，日志中的测试入口为 `target/debug/deps/`。
本地断言包括完整 `enqueue → perform → deliver` explore Flow 及接线标注、
移除注册后 sync 删除边且节点数不变、命名空间歧义、动态与未注册负例、
跨应用隔离、延时派发和绝对名称。注册测试还检查重复索引节点稳定性。
本次仅核对既有日志、实现和测试断言，运行文档清单检查及 `git diff --check`，
没有运行 Cargo，不声明全套回归通过。

依赖批次 274–282（任务 43–51）的九项外部验收按用户 2026-11-03 修订
以“用户取消验收”收口，不再作为本文静态支持范围的完成门槛。
[取消验收清单](../benchmarks/upstream-matrices/sidekiq.json) 已由 `fa3cb798`
归档，`cases=[]`、`external_experiments_executed=false`；上游合成 fixture
不构成固定版本的小中大九案例真实仓库矩阵。九项均未执行，没有案例 summary
或原始实验日志；真实仓库回放、重复 A/B、指定模型、性能指标及 VM/Docker
验证均未执行，不声明外部验收通过。


<a id="mediatr-static-coverage"></a>
### MediatR 静态覆盖收口（2026-11-05）

沿用 C# `.cs` 提取，不新增语言或扩展名。AST 注册要求非开放泛型类直接
声明泛型 handler 接口（名称以 `I` 开头、以 `Handler` 结尾），接口第一个
类型参数与直接实例 `Handle` 方法的第一个参数类型拼写完全一致，且方法
具有方法体；不注册 static、abstract 方法或嵌套类中不属于该声明的方法。
支持限定接口与请求类型、自定义 handler 接口；不验证接口实际来自 MediatR
包或 DI 注册，不新增 handler 节点，不进行别名或短名称归一化。

派发只接受方法或函数体内的成员 `Send`/`Publish` 调用，第一个实参必须是
内联 `new` 构造，类型拼写与注册精确匹配。接收器只能由标识符、this 和
成员访问组成，文本忽略大小写后须包含 mediator、sender 或 publisher；
这是名称启发式，不是接收器类型证明。同一作用域内 `Send` 要求唯一目标，
`Publish` 连接全部匹配目标，不额外区分 request/notification 接口种类。
已有普通调用边继续连接 handler 的静态 helper。

作用域优先取最近的 `.sln`/`.slnx` 所在目录，无 solution 时取最近的
`.csproj` 目录，再回退到顶层应用目录；不读取 solution 项目成员关系。
每文件最多连接 80 个不同来源/目标对，边带 heuristic、`mediatr-dispatch`
与派发位置 `registeredAt`。不承诺变量实参、参数传递、mapper 返回类型、
计算接收器、继承注册、开放泛型、DI/pipeline behavior 或运行时执行顺序；
未注册、跨作用域或 Send 同名歧义保持无合成边。

只读上游 HEAD 已核对为 `15483713763f344ff30fb7df92bcf091c7bd9758`。
注册与派发提交为 `ba735527`、`8dc34a2a`，当前实现与两份测试相对派发
提交无差异。复核既有 `/tmp/rcg285-final.log` 中的非零本地测试结果：
注册、派发各 4 项通过，合计 8 passed、0 failed、0 ignored，
测试入口为 `target/rust-slot-1/debug/deps/`。
断言覆盖 `Entry → Handle → Finish` 完整 explore Flow 与接线标注、
重复索引节点稳定、移除注册后 sync 删除边、Publish 扇出、Send 歧义、
动态与未注册负例以及 solution 隔离。本次只复核既有日志与源码，运行文档
清单检查和 `git diff --check`，未运行 Cargo，不声明全套回归通过。

依赖批次 287–295（任务 56–64）的九项外部验收按用户 2026-11-03 修订
以“用户取消验收”收口，不再作为静态支持说明的完成门槛。
[取消验收清单](../benchmarks/upstream-matrices/mediatr.json) 由 `f3624b4c`
归档，`cases=[]`、`external_experiments_executed=false`。上游合成 fixture
不是固定版本的小中大九案例真实仓库矩阵；九项均未执行，没有案例 summary
或原始实验日志。真实仓库回放、重复 A/B、指定模型、性能指标及 VM/Docker
验证均未执行，不声明外部验收通过。


<a id="spring-event-static-coverage"></a>
### Spring 事件静态覆盖收口（2026-11-05）

沿用 Java `.java` 提取，不新增语言或扩展名。AST 注册要求带方法体的
`@EventListener` 或 `@TransactionalEventListener` 方法，注解可用 Spring
完整限定名；方法必须恰有一个普通事件参数，其类型为简单或限定名称。
不接受数组、可变参数、泛型或基本类型；注解内的 classes/value 不用于推断
事件类型，无参数注解监听器及 `ApplicationListener` 接口不在本次覆盖内。
短注解名是语法识别，不验证实际 Spring 导入、bean 注册或静态方法合法性。
注册复用已有方法节点，不新增监听器节点。

派发要求方法或函数体内显式接收器的 `publishEvent` 调用，唯一实参为内联
`new Event(...)`，不接受匿名类。接收器只由标识符、this 和字段访问组成，
不证明其实际为 Spring publisher。事件类型按完整限定名、唯一显式导入或
当前 package 解析；无显式匹配且存在 wildcard 导入时保持无边，冲突导入
也不连边。同一事件连接作用域内全部匹配监听器，已有普通调用边连接静态 helper。

应用作用域取最近的 `pom.xml`、`build.gradle`、`build.gradle.kts`、
`settings.gradle` 或 `settings.gradle.kts` 所在目录，无标记时回退到顶层目录；
不读取构建文件成员关系。每文件最多连接 80 个不同来源/目标对，合成边带
heuristic、`spring-event`、事件类型 `via` 和派发位置 `registeredAt`。
不承诺变量实参、计算接收器、继承事件类型匹配、跨模块接线、Kotlin 监听器、
条件表达式求值、异步投递或事务执行顺序；事务注解仅作为静态注册证据。

只读上游 HEAD 已核对为 `15483713763f344ff30fb7df92bcf091c7bd9758`。
注册与派发提交分别为 `d3544a77`、`7ee591d1`，当前实现及两份测试相对
派发提交无差异。复核既有 `/tmp/rcg298-green.log` 的派发 5 项通过，以及
`/tmp/rcg298-regression.log` 的注册 4 项、MediatR 回归 4 项和 explore
预算回归 28 项通过：合计 41 passed、0 failed、0 ignored，测试入口为
`target/rust-slot-1/build/debug/deps/`。断言包括完整
`Entry → Receive → Finish` explore Flow 与接线标注、重复索引节点稳定、
移除注解后 sync 删除边、扇出、包名与显式导入、Maven/Gradle 应用隔离，
以及动态、未注册、注释/字符串、wildcard 和冲突导入负例。
本次仅复核既有日志、实现和测试断言，执行文档清单检查及 `git diff --check`，
未运行 Cargo，不声明全套回归通过。

依赖批次 300–308（任务 69–77）的九项外部验收按用户 2026-11-03 修订
以“用户取消验收”收口，不再作为静态支持说明的完成门槛。
[取消验收清单](../benchmarks/upstream-matrices/spring-event.json) 已由
`0a8f9288` 归档，`cases=[]`、`external_experiments_executed=false`。
上游合成 fixture 不是固定版本的小中大九案例真实仓库矩阵，且其接口监听器
等断言不代表本地 Rust 支持。九项均未执行，没有案例 summary 或原始实验日志；
真实仓库回放、重复 A/B、指定模型、性能指标及 VM/Docker 验证均未执行，
不声明外部验收通过。


<a id="laravel-event-static-coverage"></a>
### Laravel 事件静态覆盖收口（2026-11-05）

沿用 PHP `.php` 提取，不新增语言或扩展名。AST 注册复用带方法体的
`handle` 方法节点，以首个参数的命名类型、nullable 或 union 命名成员作为
事件证据；也识别类属性 `$listen` 的字面量数组映射，事件和监听器可用
`Type::class` 或只含类名字符的字符串。映射目标须唯一，可连接无类型参数的
`handle`。这是语法启发式，不验证 Laravel 安装、provider 继承、容器注册或
监听器实际启用情况；字符串也按类型引用解析，不模拟 PHP 字符串转义求值。

派发只识别函数或方法体内 `event(new Event(...))` 与
`\event(new Event(...))`，要求唯一实参和静态类名。按完整限定名、显式
use 别名、当前 namespace 解析类型并忽略大小写；支持分号及花括号 namespace。
分组 use 和冲突别名保持无边。应用范围取最近 `composer.json` 所在目录，
无标记时回退到顶层目录，不读取 Composer autoload 或依赖关系。
同一事件连接范围内全部匹配监听器，普通静态调用边继续连接 helper。
每文件最多 80 个不同来源/目标对；合成边带 heuristic、`laravel-event`、
事件类型 `via` 和派发位置 `registeredAt`，不新增节点。

不承诺变量事件、动态类名、Event facade、静态 dispatch、运行时订阅、
事件继承匹配、跨应用接线、队列投递或执行顺序。普通同名 `event` 函数和
同名类属性也可能满足语法规则，不应将合成边理解为运行时注册证明。

只读上游 HEAD 核对为 `15483713763f344ff30fb7df92bcf091c7bd9758`。
注册与派发提交分别为 `5c9b3902`、`095f69cf`，派发实现及测试相对后者无差异。
复核既有 `/tmp/rcg310-final.log` 的注册 4 项通过，以及
`/tmp/rcg311-final.log` 的 Spring 回归 5 项、注册 4 项、派发 4 项通过，
各组均为 0 failed、0 ignored；测试入口为 `target/rust-slot-1/debug/deps/`。
派发断言包括完整 `entry → handle → finish` explore Flow 与接线标注、
重复索引节点稳定、sync 删除过期边、provider 映射、应用和 namespace 隔离、
union 类型，以及动态、未注册、无关调用和歧义映射负例。
本次只复核既有日志、实现与断言，执行文档清单检查和 `git diff --check`，
未运行 Cargo，不声明新一轮行为测试或全套回归通过。

依赖批次 313–321（任务 82–90）的九项外部验收按用户 2026-11-03 修订
以“用户取消验收”收口，不再作为静态支持说明的完成门槛。
[取消验收清单](../benchmarks/upstream-matrices/laravel-event.json) 已由
`c7c3cf24` 归档，`cases=[]`、`external_experiments_executed=false`。
上游合成 fixture 不是固定版本的小中大九案例真实仓库矩阵；九项均未执行，
没有案例 summary 或原始实验日志。真实仓库回放、重复 A/B、指定模型、
性能指标及 VM/Docker 验证均未执行，不声明外部验收通过。
