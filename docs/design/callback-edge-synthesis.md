# 设计+状态：通用回调/观察者边缘综合

**状态：** 已发货（`callback_synthesizer.rs` 中的合成器已合并并在
`main`）。该文档记录了原始设计。
**动机：** 关闭静态提取留下的动态调度漏洞
观察者/事件发射器/信号模式，其中*调度程序*调用回调
通过共享商店在其他地方注册 - 所以流程就像“更新如何
到达屏幕”实际上存在于图中。

> **更新 (2026-06-01)：** `trace` 和 `context` MCP 工具
> 自**删除**以来，`rustcodegraph_explore` 现在是单一曲面工具。它是
> “流动”部分 (`format_flow_section`) 和 `rustcodegraph_node` 步道表面
> 这些合成的边缘；下面的 `trace(a, b)` 符号表示“a→b 流”，
> 您现在使用 `rustcodegraph_explore` / `rustcodegraph agent-eval probe-explore` 进行验证
> （旧的跟踪/上下文开发探针随工具一起消失了）。

---

## TL;DR 新会话

我们合成静态解析遗漏的 `dispatcher → callback` 边缘。有用：

- **现场观察员** (excalidraw `Scene.onUpdate`/`triggerUpdate`)：合成
`triggerUpdate → triggerRender`。现在 `trace(mutateElement, triggerRender)` = 3 跳。
- **EventEmitter**（表示`on('mount', …)`/`emit('mount')`）：合成`use → onmount`。
- 精度高：Excalidraw 从 27k 条合成边缘中得到了 **1** 条（正确的边缘）；
第 3 阶段后节点数增加了 +3（没有爆炸）。

**触及的文件：**
- `src/resolution/callback_synthesizer.rs` — 全图综合阶段（阶段 1 + 2）。
- `src/resolution/index.rs` — 在保留基边后调用 `synthesize_callback_edges()`。
- `src/extraction/tree_sitter.rs` — `visit_function_body` 现在提取**命名**嵌套
函数（第 3 阶段），因此内联命名处理程序成为可链接节点。

**如何重现/测试：**
```bash
npm run build
rm -rf /tmp/rustcodegraph-corpus/excalidraw/.rustcodegraph
( cd /tmp/rustcodegraph-corpus/excalidraw && rustcodegraph init -i )
# synthesized edges (provenance='heuristic', metadata.synthesizedBy in {callback,event-emitter}):
sqlite3 /tmp/rustcodegraph-corpus/excalidraw/.rustcodegraph/rustcodegraph.db \
  "select s.name||' → '||t.name||'  '||coalesce(e.metadata,'') from edges e \
   join nodes s on e.source=s.id join nodes t on e.target=t.id where e.provenance='heuristic';"
# end-to-end flow (the synthesized edge shows up in explore's Flow section + node trail):
rustcodegraph agent-eval probe-explore /tmp/rustcodegraph-corpus/excalidraw "triggerUpdate triggerRender"
```
Rust 探针命令：`rustcodegraph agent-eval probe-node`（符号+踪迹），
`rustcodegraph agent-eval probe-explore`（相关来源+命名符号之间的流程）。事件发射器
固定装置位于 `/tmp/cb-fixture/bus.js`（短暂的 - 重新创建或移至 `__tests__/`）。

---

## 洞

```ts
class Scene {
  private callbacks = new Set<Callback>();
  onUpdate(cb: Callback) { this.callbacks.add(cb); }          // REGISTRAR
  triggerUpdate() { for (const cb of this.callbacks) cb(); }  // DISPATCHER
}
this.scene.onUpdate(this.triggerRender);                      // REGISTRATION SITE
```

运行时边 `triggerUpdate → triggerRender` 并不静态存在：
`triggerUpdate` 唯一的字面调用是 `cb()`（匿名）。测量：`triggerUpdate`
唯一的被调用者是 `randomInteger`； `trace(triggerUpdate, triggerRender)` 没有返回路径。

## 为什么它是全图传递，而不是 `FrameworkResolver.resolve()`

`resolve(ref)` 回答“这个**命名**引用指向什么”，一次一个引用。这
回调边缘**没有要解析的引用**（`cb()` 是匿名的）并且需要**跨文件，
多站点关联**（注册商、注册、调度员）。所以这是一个全图
在基本解析之后通过，语言级别（任何 OO 观察者），生活在
`src/resolution/callback_synthesizer.rs` — **不**在 `frameworks/` 下。

> *其他*动态调度类的同级机制 - **命名**属性/
> 描述符调度（例如 django `self._iterable_class(...)`）——是
> `claims_reference`挂钩（`resolution/types.rs`+`resolution/index.rs`前置过滤器）
> + `FrameworkResolver.resolve()`（`frameworks/python.rs` 中的 django ORM 解析器）。
> 这个*确实*适合 `resolve()`，因为 ref 已命名。两者都是同一部分的一部分
> 覆盖努力；请参阅“相关工作”部分。

---

## 竣工算法（以及与原始设计的差异）

### 现场观察员通道（`fieldChannelEdges`，第一阶段）
1. **候选人** 按方法/函数 **名称** — 注册商 `^(on[A-Z]\w*|subscribe|
addListener|addEventListener|注册|监视|监听|addCallback)$`;调度员
包含 `(emit|trigger|notify|dispatch|fire|publish|flush)`。
2. **通过正文确认**（通过 `ctx.readFile` + 切片节点行读取）：注册商已
`this.<F>.add|push|set(`;调度员有 `for (… of [Array.from(]this.<F>)` + 一个电话，
或 `this.<F>.forEach(`。
3. **配对 — 分歧：** 设计表示按 *class* 配对；构建对
**相同文件 + 相同字段 `F`** （文件作为类代理 — 获取包含类
可靠地更难）。适用于常见的每文件 1 类情况；重新访问
多类文件。
4. **注册：** `queries.getIncomingEdges(registrar.id, ['calls'])` → 对于每个，
在边缘线读取调用者的源代码并**正则表达式恢复arg**
(`<registrarName>\s*\(\s*(?:this\.)?(\w+)`)。分歧：设计首选树保姆
重新解析；构建使用正则表达式（仅命名引用 - 此处缺少箭头/内联参数）。
5. **综合** `dispatcher → fn`（`getNodesByName(arg)`→方法|函数）。上限为
`MAX_CALLBACKS_PER_CHANNEL = 40`。

### EventEmitter 通道（`eventEmitterEdges`，第 2 阶段）
- **面向文件的扫描**（`ctx.getAllFiles()` + `readFile`，子字符串预过滤器开启
`.emit(`/`.on(`/等）。 `ON_RE` = `\.(?:on|once|addListener)\(\s*[’”]([^'"]+)['"]\s*,\s*
(?:function\s+(\w+)|(?:this\.)?(\w+))`; `EMIT_RE` = `\.(?:emit|fire|dispatchEvent)\(\s*[’”]([^'"]+)['"]`。
- Dispatcher = `emit('e')` 调用的 **封闭函数**（`enclosingFn` 找到
包含该行的最紧密函数/方法/组件节点）。处理程序 = `getNodesByName`
处理程序名称的。
- 通过**事件名称文字**进行关联；综合调度程序→处理程序。
- **精度 — 发散：** 设计建议的接收器类型匹配；构建使用
**事件扇出上限** (`EVENT_FANOUT_CAP = 6`) — 跳过具有 >6 个处理程序的事件或
调度程序（像 `error`/`change` 这样的通用名称会在没有类型信息的情况下过度链接）。

### 出处——分歧
`Edge.provenance` 是一个固定枚举（`'tree-sitter'|'scip'|'heuristic'`），如此综合
边使用 **`provenance: 'heuristic'`** + `metadata: { SynthesisBy: 'callback'|
'事件发射器'，via/event/field }`. The design's `'callback-synthesis'`出处和
高/中/低 **未实施置信层** — 扇出上限 +
相反，注册商名称唯一性+仅命名处理程序是精确防护。

### 第 3 阶段 — 内联回调提取 (`tree_sitter.rs`)
真实存储库上 EventEmitter 的真正拦截器：内联处理程序
(`on('mount', function onmount(){})`) 不是**节点**，因此没有任何东西可以链接到它们。
根本原因：`visit_function_body` 遍历了嵌套函数而不提取它们。
修复：在调用/结构访问者中，当主体节点是类函数节点并且
名称提取返回真实姓名，提取函数（提取它并遍历
它自己的身体）并返回。 **仅命名** — 匿名箭头落入现有的
递归（因此它们的内部调用仍然归因于封闭的 fn）。这限制了它：
excalidraw +3个节点，不爆炸，不回归。

---

## 验证结果（实际）

| 回购协议 | 结果 |
|---|---|
| 外画 | 1 条合成边 `triggerUpdate → triggerRender`（共 27,214 条）； `trace(mutateElement, triggerRender)` = 3 跳；节点 9,286 → 9,289 |
| 表达 | 第 3 阶段之后：`use → onmount` `{event-emitter, event:"mount"}`（`onmount` 现在在 `application.js:109` 处提取） |
| `/tmp/cb-fixture/bus.js` | `tick → handleRefresh`、`persist → handleSave`（命名方法 EventEmitter 处理程序） |
| exalidraw / 快递 | 无第一阶段回归；节点数稳定 |

---

## 剩余工作（优先考虑下一次会议）

1. **匿名箭头处理程序** - `on('e', () => foo())` 仍然不产生边（没有节点，
故意不在第 3 阶段提取）。修复方法是**合成器链接到主体**：
解析箭头的主体并链接 `dispatcher → (calls inside the arrow)`。最高
剩余召回获胜；处理最常见的现代回调形态。
2. **连接到 `resolveAndPersist`**（增量同步）- 综合当前仅运行
在 `resolveAndPersistBatched`（完整索引）中。增量重新索引不会刷新
合成边缘。
3. **接收器类型匹配**用于EventEmitter精度（替换/增强扇出
cap) — 使用 `type_of` 边，因此 `x.emit('change')` 仅链接到 `y.on('change', fn)`
当`x`、`y`为同一类型时。让扇出帽放松。
4. **Tree-sitter arg 恢复**（替换现场通道第 4 阶段中的正则表达式）- 稳健
箭头、多参数、换行调用。
5. **单回调字段** (`this.onChange = cb; … this.onChange()`) — 标量存储
现场观察员的变体；没有建成。
6. **广泛的精确度/召回率审计**——遍历整个语料库；统计合成边缘
对每个存储库进行抽查，确认 EventEmitter 密集的存储库没有爆炸。
7. **测试 + 变更日志** — 该装置是合成器的现成 vitest 案例；添加
第 3 阶段的提取器测试（named-nested-fn 提取；确认其他语言
不受影响 - 更改发生在共享 Walker 中），解析器测试 django 端。

## 边缘案例/模型
- **跨实例的过度近似**被接受（可达性，而不是实例
精确）。 `unregister`/`off` 被忽略。
- 合成的边是**可加的**——永远不会替换静态边；工具可以过滤
`provenance='heuristic'` + `metadata.synthesizedBy`。

## 相关工作（相同的覆盖范围）
这是关闭动态调度覆盖范围的一半。 `main` 上的其他工件：
- **命名属性/描述符解析器**：`claims_reference`（`resolution/types.rs`，
`resolution/index.rs` 中的预过滤器）+ django ORM 解析器（`frameworks/python.rs`，
`_iterable_class` → `ModelIterable.__iter__`）。
- **检索/UX 更改**（与覆盖范围分开）：`explore` 整个小文件 + 粘合
修复了 `explore` 流动部分 (`format_flow_section`) 和 `node`-with-trail
— 全部在 `src/mcp/tools.rs` 中。 （`trace` / `context` 后来
删除； explore 是唯一的表面处理工具。）
- **完整的调查背景+结果：**自动记忆
`project_rustcodegraph_read_displacement`（为什么覆盖 - 不提示/钩子/新工具 -
是让代理使用 RustCodeGraph 而不是 Read 的杠杆）。
