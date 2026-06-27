# AGENTS.md

本文件为 Codex（Codex.ai/code）在本仓库中工作时提供指导。

## 项目概览

RustCodeGraph 是一个本地优先的代码智能库 + CLI + MCP 服务器。它使用 tree-sitter 解析任意受支持的代码库，将符号、边、文件存入 SQLite（FTS5），并通过 MCP 将知识图谱暴露给 AI 代理（Codex、Cursor、Codex CLI、opencode）。每个项目的数据都存放在 `.rustcodegraph/` 中。提取过程是确定性的，来源于 AST，而不是 LLM 摘要。

它以 `rustcodegraph` 的名称发布到 npm；同一个二进制同时承担安装器、索引器和 MCP 服务器的职责。

## 构建、测试、运行

```bash
npm run build           # cargo build --release
npm run dev             # cargo check
npm run clean           # cargo clean

npm test                # cargo test
npm run test:eval       # cargo test --test evaluation_types
npm run eval            # cargo test --test evaluation_types

npm run cli             # cargo run --bin rustcodegraph --

# 单个测试文件 / 模式
cargo test --test installer_targets_test -- --nocapture
cargo test --test extraction_test TypeScript -- --test-threads=1
```

npm 包现在只是 Rust 二进制外面的一层轻量 npm 启动器。
根级别的测试与构建由 Rust 侧负责；站点和遥测等旁支项目保留各自包级别的工具链。

## 架构

### 分层流水线

```
files → ExtractionOrchestrator (tree-sitter) → DB (nodes/edges/files)
              ↓
       ReferenceResolver (imports, name-matching, framework patterns)
              ↓
       GraphQueryManager / GraphTraverser (callers, callees, impact)
              ↓
       ContextBuilder (markdown/JSON for AI consumption)
```

对外公开的 Rust API 入口是 `src/index.rs`；它把提取、数据库、图、上下文、同步和面向 MCP 的各层串起来。库使用者只需要接触这一个文件；MCP 服务器和 CLI 也都通过它驱动。

### 模块布局

- `src/index.rs`：Rust 门面层，包含 `init_sync`/`open_sync`/`close`、`index_all_sync`、`sync`、`search_nodes`、`get_callers`/`get_callees`、`get_impact_radius`、`build_context`、`watch`/`unwatch`。
- `src/db/`：SQLite 连接、预编译查询、schema、原生/wasm 风格适配器，以及状态报告。
- `src/extraction/`：提取编排器、tree-sitter 封装、`languages/` 下按语言划分的提取器（每种语言一个文件），以及面向非 tree-sitter 格式的独立提取器（`svelte_extractor.rs`、`vue_extractor.rs`、`liquid_extractor.rs`、用于 Delphi 的 `dfm_extractor.rs`）。`parse_worker.rs` 负责把重解析工作放到主线程之外运行。
- `src/resolution/`：`ReferenceResolver` 负责协调 `import_resolver.rs`（以及用于 tsconfig 路径别名和 cargo workspace member glob 的 `path_aliases.rs`）、`name_matcher.rs` 和 `frameworks/`（Express、Laravel、Rails、FastAPI、Django、Flask、Spring、Gin、Axum、ASP.NET、Vapor、React Router、SvelteKit、Vue/Nuxt、Cargo workspaces）。这些框架会产出 `route` 节点和 `references` 边。
- `src/graph/`：`GraphTraverser`（BFS/DFS、影响半径、路径查找）和 `GraphQueryManager`（高层查询）。
- `src/context/`：`ContextBuilder` 以及 markdown/JSON 输出格式化器。
- `src/search/`：全文查询解析器和 FTS5 辅助工具。
- `src/sync/`：`FileWatcher`（原生 FSEvents/inotify/RDCW）与防抖 + 过滤，以及 git hook 辅助工具。
- `src/mcp/`：MCP 服务器、daemon/proxy/session 处理、工具分发、传输类型，以及返回 MCP `initialize` 指导文本的 `server_instructions.rs`。
- `src/installer/`：见下文。
- CLI 二进制源码：`src/bin/rustcodegraph.rs`，构建并发布为 `rustcodegraph`。子命令包括：`install`、`init`、`uninit`、`index`、`sync`、`status`、`query`、`files`、`context`、`affected`、`serve --mcp`。
- `src/ui/`：终端 UI（shimmer 进度、worker）。

### NodeKind / EdgeKind

定义在 `src/types.rs` 中。提取器和解析器都必须使用这些精确字符串。

- **NodeKind**：`file`、`module`、`class`、`struct`、`interface`、`trait`、`protocol`、`function`、`method`、`property`、`field`、`variable`、`constant`、`enum`、`enum_member`、`type_alias`、`namespace`、`parameter`、`import`、`export`、`route`、`component`。
- **EdgeKind**：`contains`、`calls`、`imports`、`exports`、`extends`、`implements`、`references`、`type_of`、`returns`、`instantiates`、`overrides`、`decorates`。

### 多代理安装器

`src/installer/` 是 `rustcodegraph install` 的入口点（以及裸调用 `rustcodegraph`/`npx rustcodegraph` 时的入口）。其架构如下：

- `targets/registry.rs` 列出所有受支持的代理。
- `targets/types.rs` 定义 `AgentTarget` trait。要新增一个代理，只需要在 `targets/` 下增加一个新文件，并在 `registry.rs` 中增加一项。每个 target 自己负责其配置文件位置，以及 MCP server JSON/TOML/JSONC 的写入。（target 不再写 instructions 文件，见下文。）
- 当前 target：`claude.rs`、`cursor.rs`、`codex.rs`、`opencode.rs`、`gemini.rs`、`hermes.rs`、`antigravity.rs`、`kiro.rs`。
- `targets/toml.rs` 是一个手写的 TOML 序列化器，仅作用于 `[mcp_servers.rustcodegraph]`（供 Codex 使用）。同级 table 和 `[[array_of_tables]]` 会原样保留，不引入新依赖。
- opencode 默认读取 `opencode.jsonc`；安装器优先使用现有 `.jsonc`，其次回退到 `.json`，全新安装时创建 `.jsonc`。编辑通过 `jsonc-parser` 做精确手术式修改，因此用户注释和格式能在 install/re-install/uninstall 往返中保留下来。
- `instructions_template.rs` 导出 `<!-- RUSTCODEGRAPH_START -->`/`<!-- RUSTCODEGRAPH_END -->` 标记和一段简短安装器文本。每个 target 的 `install` 和 `uninstall` 都通过这些标记，在写入当前 RustCodeGraph 配置前先移除旧的受管内容块。
- 所有安装器改动都需要在 `tests/installer_targets_test.rs` 中有对应覆盖。这里有参数化契约测试，覆盖 install 幂等性、同级内容保留、uninstall 可逆、字节级完全相同的重复运行返回 `unchanged`，以及 Codex 的部分状态恢复。

### Cursor MCP 工作目录怪癖

Cursor 启动 MCP 子进程时会使用错误的 cwd，并且不会在 `initialize` 中传入 `rootUri`。安装器会把 `--path` 注入到 Cursor 的 MCP 参数里：本地安装用绝对路径，全局安装用 `${workspaceFolder}`。如果你修改 Cursor 接线逻辑，必须保留这一点。

### MCP 服务器说明文本

`src/mcp/server_instructions.rs` 会在 MCP `initialize` 响应里返回给代理。这是每个代理最先看到的工具使用说明。若要改工具指导，先改这里；如果相关规则变化了，也要保持 dogfooding 用的 Cursor 规则同步。

## 检索性能与动态分发覆盖（不要回退）

RustCodeGraph 的核心价值，是让代理用少量 **快速** 的 rustcodegraph 调用，并且 **零 Read/Grep**，就能回答 **结构/流程** 类问题（“X 怎么到 Y”“链路追踪”“影响分析”“调用者”）。优化目标是 **墙钟延迟 + 工具调用次数**，不是 token 成本。此前的描述把成本说成“持平”并不准确，实际上它是 **更低** 的：当前版本在 README 那 7 个仓库上做 with-vs-without A/B，对 4 次运行取中位数，平均可节省 **35% 成本 · 57% token · 46% 时间 · 71% 工具调用**，与已发布 README 一致。原因是 **轮次数量大幅减少，总累积上下文也明显更小**，而不是缓存性更强：without 分支的高 token 量大部分是便宜的缓存读取，所以 token 降幅（57%）会比成本降幅（35%）更大。统计 token 时要 **累加每一轮 assistant usage**，而不是读 `result.usage`（当前 Codex 里它只反映最后一轮）。参见 `docs/benchmarks/call-sequence-analysis.md`。这里最重要的机制是：**一旦 rustcodegraph 的回答不够，代理立刻就会回退到 Read/Grep。** 所以所有改动都只用一个标准判断：rustcodegraph 的回答是否足够完整，足以 **阻止** 代理再去读文件？

**目标行为：** 一个流程问题在小仓库里应当 **1 次 rustcodegraph 调用** 就解决，大仓库放宽到 **3–5 次**，同时 **Read/Grep = 0**。审 PR 或尝试新方案时，不能让这个目标退化。

### 让工具适配代理，不要试图改造代理

这是决定检索改动是否值得落地的杠杆。**在这里动手之前先验证：它是不是让代理 _已经会调用_ 的工具，在 _已有输入_ 下做得更多？如果反过来要求代理改变行为，比如换个工具、换种查询写法、靠示例学习，那它就会撞上低显著性墙，最终落不了地。**

RustCodeGraph 能影响代理的通道只有低显著性的两类：MCP `initialize` 说明（`server_instructions.rs`）和工具描述。改这些内容 **并不能可靠地** 改变代理的工具选择或查询风格。新工具如果代理本来就不爱选，会更糟；“提供更好的示例”本质上也是同一个引导问题。随着宿主模型的工具使用能力进步，代理的选工具能力会自己变好，但那不是我们能强迫发生的。

真正有效的方法，是在代理已经会做的事情上接住它：
- **explore-flow**：`rustcodegraph_explore` 是代理最稳定会调用的主工具；它的查询输入是一组精确的符号名（包括 `Class.method` 这种限定名），覆盖代理正在追踪的那条流程；explore 会在这些命名符号之间找到调用路径（借助合成边），并把它放在输出最前面。（`format_flow_section` 负责处理分段/同名消歧；最多允许 1 个未命名桥接节点，防止它在巨型函数的扇出里迷路。它还具备重载感知：查询里出现 PascalCase 类型 token 时，会把同名重载优先偏向那个类型自己的定义，例如 `DataRequest task` 会指向 DataRequest 的 `task`，而不是抽象基类；同时优先排序含有命名符号的文件。）
- **充分性**：工具输出必须完整到足以让代理停下。`rustcodegraph_node` 会返回完整函数体 + caller/callee 链；对于歧义名称，它会 **一次性返回所有重载的函数体**，这样代理就不用再去 Read 文件找正确的重载了，这一点已在 Alamofire/gin 上验证。它是 explore 之后继续下钻的深度工具（标记为 SECONDARY）。
- **错误会教会代理放弃**：如果一个会话早期连续收到一两次 `isError: true`，代理通常就会完全停止调用 rustcodegraph，这是维护者反复观察到的现象。`isError` 只能留给真正“该停止尝试”的场景：安全拒绝（`PathRefusalError`）和真实故障（并且要带“可重试一次”的提示）。所有可预期/可恢复的情况，比如项目未索引、找不到符号、文件不在索引内，都要返回 **成功形状的响应，并把指导信息放进去**（如 `NotIndexedError` → `textResult`，见 `ToolHandler.execute` 的 catch 分支）。这个原则也适用于整个会话：**未索引工作区应返回空的 `tools/list` + 两行“未激活”说明**，而不是暴露 8 个会全部失败的工具。对代理而言，“没有工具可用”是唯一不会被误读的信号；是否索引，应始终由用户决定，而不是代理替用户做决定。

反例则恰恰相反：把精确答案塞进一个 **模糊输入** 工具里。已经被移除的 `context` 接收的是描述而不是符号，因此它没法区分一条流程的端点，常常返回 **错误的功能点**，所以才被删除。精确输出必须建立在精确输入之上，explore 之所以接收 symbol bag，就是这个原因。（`trace` 也是同理被移除：explore-flow 已经覆盖了它的职责，而代理本来也不怎么选它。）

这一方向剩下真正有杠杆的点只有 **覆盖率**：任何能在静态层面打通的流程，比如新增一个动态分发合成器，或把静态解析漏掉的符号补抽出来，例如 `create((set,get)=>({...}))` 里的 object-literal store actions，都会自动被 explore-flow 呈现出来，不需要改变代理行为。Reactive/reconciler 这类运行时（Halo 的 `ReactiveExtensionClient`、MediatR、Vue Proxy）仍然是前沿区域：这些流程没有静态边，因此现在就不应显示任何结果，沉默比错误更好。完整调查和 A/B 记录见 `docs/benchmarks/call-sequence-analysis.md` 以及自动记忆 `project_rustcodegraph_read_displacement`。

### Explore 预算：两个预算都必须随着仓库规模单调增长

`src/mcp/tools.rs` 中有两个函数，会按索引文件数来放大 explore 的预算。预期分辨率如下，任何回退都会悄悄把代理重新推回 Read：

| 仓库 | 文件数 | explore 次数 | 每次字符数 | 单文件上限 |
|---|---|---|---|---|
| express（小） | 147 | 1 | 18K | 3800 |
| excalidraw/django（中） | 643–3043 | 2 | 28K | 6500 |
| vscode（大） | 10446 | 3 | 35K | 7000 |
| ~20k / ~40k | — | 4 / 5 | 38K | 7000 |

- `getExploreBudget(fileCount)`：**调用次数** 预算：`<500→1, <5000→2, <15000→3, <25000→4, ≥25000→5`（最大 5）。
- `getExploreOutputBudget(fileCount)`：**每次调用输出** 预算（字符数 / 文件数 / 单文件上限）。**不变量：更大的档位，`maxCharsPerFile` 绝不能比更小档位还小。**（促使本节出现的那个回归，就是 `<5000` 档的 2500 竟然低于 `<500` 档的 3800，导致在像 excalidraw 的 415 KB `App.tsx` 这种巨型文件仓库里，一次 explore 只能返回不到 1% 的内容，最终迫使代理去 Read。）
- Explore 的输出 **永远不要告诉代理“去用 Read”**，而应当把它引导到下一次 `rustcodegraph_explore`，并明确“返回的源码应视为已经 Read 过”。

### 动态分发覆盖：整条流程必须在图里首尾连通

静态 tree-sitter 提取无法覆盖 computed/indirect calls，所以流程会在动态分发处断掉，代理就只能靠读文件把它补起来。合成器/解析器的职责，就是把这些边补上，让 `rustcodegraph_explore` 能从头到尾把流程接起来（见 `src/resolution/callback_synthesizer.rs`、`src/resolution/frameworks/`）。当前支持的通道包括：callback/observer、EventEmitter、**React re-render**（`setState`→`render`）、**JSX 子组件**（`render`→child component）、django ORM descriptor。所有合成边都标记为 `provenance:'heuristic'`，并带有 `metadata.synthesizedBy` + `registeredAt`（边被接入的 wiring 位置）；这些信息会直接内联出现在 `rustcodegraph_explore` 的 Flow 段和 `rustcodegraph_node` 的 trail 中。

**原则：部分覆盖比不覆盖更糟。** 只补一个边界、不补下一个边界，会让代理看到一个中间跳点，然后继续往下钻并读文件收尾。对 excalidraw 的测量表明：只加 react-render 反而会把 reads 提高到 5–7；只有把整条链补完整，再加上 jsx-child 这一步，reads 才会降到 0–1。**必须始终把整条流程首尾接通后再重新测量**，绝不能交付“只桥接了一半”的流程。

### 验证方法（每个新语言/框架都必须执行）

对每个 **语言 × 框架**，都要在 **小、中、大** 三类真实仓库上验证，并且每类至少有 **3 个不同的流程问题**：

1. **选出该框架的典型流程**：例如“X 如何到达 Y”，可能是 state→render、request→handler→view、query→SQL、action→reducer→store 等。
2. **确定性探针**：使用 Rust 二进制上的 `rustcodegraph agent-eval probe-node` / `rustcodegraph agent-eval probe-explore`。要求 `rustcodegraph_explore` 在给出那条流程涉及的符号名后，能从起点到终点完整连通，中间没有断点（Flow 区必须展示该路径）；同时 **不能发生节点爆炸**（`select count(*) from nodes` 在重建索引前后保持稳定）；并对合成边的 **精度** 做 spot-check（`select … where provenance='heuristic'`）。
3. **Agent A/B**：运行 `scripts/agent-eval/run-all.sh <repo> "<Q>"`，比较 with rustcodegraph 和 without rustcodegraph，并且 **每个分支至少 2 次运行**（运行间波动很大，绝不能 n=1 就下结论）。记录 **耗时、总工具调用、Read、Grep**。如果需要证明充分性，也可以生成一个临时 Claude settings 文件，把 `scripts/agent-eval/block-read-hook.sh` 接到 `PreToolUse(Read)` 上，强制验证 Read=0。
   - **模型策略：每个 A/B 分支都必须用 Codex 的 `--model sonnet --effort high`。始终如此，不能换 Opus/Fable。** `scripts/agent-eval/*.sh` 默认就是这样（允许通过 `MODEL`/`EFFORT` 环境变量覆盖，但除非维护者明确要求，否则不要调高）。原因有两个，第二个更重要：一是 Sonnet 不会烧太多 token；二是 **Sonnet 是刻意选择的能力下限模型**。rustcodegraph 的真实用户，会把它接到各种他们已有的代理上（Cursor Composer、Gemini 等），所以我们故意在一个“更笨”的模型上验证：更强的模型会掩盖弱模型暴露出来的 salience/sufficiency 问题。一个在 Sonnet 上成立的 affordance，通常能向上泛化到所有宿主；而只在 Opus/Fable 上成立的东西，并不能向下泛化到大多数用户真正会用到的代理。两个分支始终要使用同一个模型。
   - **MCP attach 是启动延迟问题，不是硬阻塞。** 在多步任务里，代理可能会在 rustcodegraph 启动完成前就一头扎进 Read/grep，于是等于整局都没用上 rustcodegraph。解决办法是：为目标仓库 **预热一个持久 daemon**（把 `RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS` 设高，并启动 `rustcodegraph serve --mcp --path <target> </dev/null &`），让 Codex 能在第一轮之前连上。不要相信 Codex 的 `init` 快照，它可能显示 `status:"pending"` / 0 tools，但之后其实连上了；要以 `rustcodegraph agent-eval parse-run` 的 `by type` 中实际有没有 rustcodegraph 使用为准。若要隔离一个改动，比较 **新构建 vs 基线构建，两边都开启 rustcodegraph**，而不是 run-all.sh 那种 with-vs-without；请使用 `scripts/agent-eval/ab-new-vs-baseline.sh <indexed-repo> "<task>" [baseline-ref]`，它内置了 pre-warm。
4. **通过标准**：一个正常的流程问题，必须在该仓库的 explore 调用预算内达到 **约 0 次 Read/Grep**，运行速度 **快于** without-rustcodegraph，并且 **在控制仓库上没有回归**。把结果记录到 `docs/design/dynamic-dispatch-coverage-playbook.md`（覆盖矩阵）中。

完整 playbook 和分机制设计见 `docs/design/dynamic-dispatch-coverage-playbook.md` 与 `docs/design/callback-edge-synthesis.md`。

### 实战示例：Excalidraw（TS/React，中型，643 个文件）

这是每种语言/框架都要复用的模板。问题是：*“更新一个 element 之后，画布是如何重新渲染到屏幕上的？”*（整条流程跨越了三个 React 边界：observer callback、`setState`→`render`，以及 JSX child）。

| 阶段 | duration | Read | Grep | rustcodegraph |
|---|---|---|---|---|
| Without rustcodegraph | 115–139s | 9–10 | 10–11 | 0 |
| Broken（explore-budget 回归） | 131–139s | 5–10 | 3–5 | 6–14 |
| Fixed（预算 + 消息 + 合成） | 64–112s | 0–2 | 2–4 | 3–**10** |
| + explore-first 引导 | **51–74s** | **0–2** | 0–4 | **3–4** |

同一问题、每阶段 n=4 次不加 hook 的运行。把流程问题优先引导到 `rustcodegraph_explore` 后：**最佳一轮是 0 Read / 0 Grep / 3 rustcodegraph / 51s**；**4 次里有 2 次完全干净**（0 Read、0 Grep）。这个引导消除了过度下钻导致的方差，调用次数从 3–10 收紧到 3–4，同时 `search`+`callers` 式路径拼装带来的失败探索也降到了 0。运行间波动仍然存在，所以报告时永远给区间，不要只给单个数。**剩余的 read/grep 都来自一次性局部数据流**（`canvasNonce`，它只是一个本地 prop，没有图边）；这是 def-use/data-flow 的前沿区域，因此刻意不覆盖，避免跟踪每个局部变量把图规模炸掉。验证结果是：`rustcodegraph_explore(mutateElement, renderStaticScene)` 能跨越三个边界，以 **6 跳** 连通（`mutateElement → triggerUpdate → [callback] triggerRender → [react-render] render → [jsx] StaticCanvas → renderStaticScene`），每一跳都展示了内联源码和 wiring 位置；节点数稳定在 9,289；合成边包括 1 条 callback、46 条 react-render、280 条 jsx-render，没有爆炸，且已做精度检查。

## 测试

测试位于 `tests/` 中，并与所覆盖模块保持镜像关系。除了显而易见的测试外，以下几个尤其重要：

- `installer_targets_test.rs`：跨代理 target 的参数化契约测试套件（见上文安装器说明）。
- `evaluation/`：Rust 评估运行器和若干 case，会在合成项目上运行 rustcodegraph 并评分；通过 `npm run eval` 运行（会先构建）。它不属于 `npm test`。
- `sqlite_backend_test.rs`：覆盖后端选择与回退逻辑。
- `pr19_improvements_test.rs`、`frameworks_integration_test.rs`：针对历史特定 PR/事故的回归覆盖；不要重命名，这些名字锚定着 git 历史。

测试通过 `fs.mkdtempSync` 创建临时目录，并在 `afterEach` 中清理。它们会写入真实文件并操作真实 SQLite，不存在 DB mock。

### Windows 条件测试

凡是有平台差异的行为，比如路径解析、盘符、`SENSITIVE_PATHS`、`%APPDATA%` 配置目录、CRLF，都必须显式加平台门控，不能靠想当然。Windows 专用断言使用 `it.runIf(process.platform === 'win32')(...)`，POSIX 专用断言使用 `it.runIf(process.platform !== 'win32')(...)`。例如 `/etc` 在 POSIX 上是敏感路径，但在 Windows 上会解析成不存在的 `C:\etc`，如果不门控，这个 `/etc` 断言在 Windows 上一定失败。Windows 侧必须做真实验证（见下文）；不要合并一个你没亲眼跑过的 Windows-gated 测试。

## 跨平台验证

开发机，以及默认的 `npm test` 目标平台，是 **macOS**，因此本地运行只覆盖 macOS 路径。另两个平台并不在本机上；如果改动涉及平台敏感行为（文件监听、socket/命名管道、路径和符号链接处理、进程生命周期、inotify 配额），必须做真实验证，不能靠猜。

### Linux（Docker）

当需要在 Linux 上测试或验证时，使用 **Docker**。虽然没有 Linux 实机，但 Docker 运行在这台 macOS 主机上。做法是从仓库构建一个一次性镜像，并在里面运行测试：

- 基础镜像用 `FROM node:22-bookworm`；`COPY` 仓库时配一个 `.dockerignore`，排除 `node_modules`/`dist`/`.git`/`.rustcodegraph`；然后执行 `RUN npm ci && npm run build`。不要复用 Mac 上的 `node_modules`，因为 `esbuild`/`rollup` 带有平台专属二进制。
- 运行时使用 **`docker run --rm --init`**。这里的 `--init` 不是可有可无的：凡是测试进程生命周期（daemon 回收、#277 的 PPID watchdog、idle-timeout）都依赖它。没有一个能回收僵尸的 PID 1 时，被 SIGKILL 或自然退出的进程会以 zombie 形式残留，而 `process.kill(pid, 0)` 仍会报告它“活着”，从而让退出检测断言假失败，尽管进程实际上已经退出。
- Linux 也是 inotify watch budget 真正会触顶的平台。统计某个进程的 watch 数，可以查看 `/proc/<pid>/fdinfo/*`，对那个 `readlink` 为 `anon_inode:inotify` 的 fd，把其中以 `^inotify ` 开头的行数累加起来。

### Windows（Parallels VM + SSH）

凡是 Windows 专属的 PR、bug 或实现，都要在真实 Windows VM 上验证，不能靠猜。连接信息存放在仓库根目录里被 gitignore 忽略的 **`.parallels`** 文件中（VM 名称、guest IP、SSH 用户/密钥）。`prlctl exec` 需要 Parallels Pro，这里不可用，所以桥接方式是 SSH。

- 从 Mac 主机连接/执行：`ssh <user>@<guest_ip> "..."`。如果要做多行操作，把 PowerShell 脚本通过 stdin 管道传进去，并且**先从注册表刷新 PATH**，因为 sshd 会话在 winget 安装后拿到的是过期 PATH：
  ```
  ssh colby@10.211.55.3 "powershell -NoProfile -ExecutionPolicy Bypass -Command -" <<'PS'
  $env:Path = [Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [Environment]::GetEnvironmentVariable("Path","User")
  Set-Location C:\dev\rustcodegraph
  PS
  ```
- 仓库要重新 clone 到 **Windows 本地路径**（`C:\dev\rustcodegraph`）并在那边执行 `npm ci`。不要在共享的 Mac 仓库上直接跑 npm，因为 `esbuild`/`rollup` 带有平台专属二进制。
- 来宾系统工具链建议通过 winget 安装：Rust stable、Node LTS、Git，以及原生 Rust 依赖所需的 MSVC build tools/redistributable。
- 如果要取贡献者 fork 上的 PR head，直接从对方 fork 拉，避开 `pull/<n>/head` 的延迟：`git fetch <fork-url> <branch>`，然后 `git checkout -f FETCH_HEAD`。
- 已知的、与当前改动无关的 Windows 既有失败，需要先确认它们在 `main` 上也能复现，再避免被它们掩盖新的回归：`security_test.rs > Session marker symlink resistance > does not follow a pre-planted symlink`（Windows 上创建 symlink 需要权限）；以及 `mcp_initialize_test.rs` / `mcp_roots_test.rs` 两组测试，它们会在 `afterEach` 删除临时目录时因为一个 `serve --mcp` 子进程仍然占着 cwd / SQLite 文件而报 `EPERM`。这是 Windows 的文件锁怪癖，不是逻辑 bug。

## 发布

本项目发布到 npm，并镜像到 [GitHub Releases](https://github.com/hunzhiwange/rustcodegraph/releases)。`CHANGELOG.md` 是唯一事实来源；GitHub Release notes 会从中提取。

### 编写 changelog 条目

**默认把条目写到 `## [Unreleased]` 下。** 这是两次发布之间所有工作的归档区。**不要为下一个版本预先创建 `## [X.Y.Z]` 区块。** Rust 的发布辅助命令（`rustcodegraph prepare-release <X.Y.Z>`）会在准备发布时，把 `[Unreleased]` 下的内容整体提升为一个新的 `## [X.Y.Z] - <YYYY-MM-DD>` 区块（如果那个版本区块已经存在，则合并进去，但你本来就不需要预建）。预先手工建版本块，正是导致 v0.9.5 发布说明过于稀疏的事故原因：一个过早加入、内容很少的 `[0.9.5]` 区块，被提取器优先选中了，压过了上方更完整的 `[Unreleased]` 内容。不要再这样做。

所有 changelog 条目都遵循以下格式规则（无论写在 `[Unreleased]` 还是历史版本下）：

1. **写面向用户、友好的说明，而不是面向工程师的内部说明。** 用 `### New Features` 和 `### Fixes` 分组（句式大小写即可）。只有当版本里真的存在时，才单独使用 `### Breaking Changes` 和 `### Security`；偏“增强/改进”的内容都归到 New Features。空分组不要保留。（这替代了旧的 Keep-a-Changelog `Added/Changed/Fixed/Removed/Deprecated` 分组方式：GitHub Release 页面会通过 `rustcodegraph extract-release-notes <X.Y.Z>` **原样提取** 每个版本块，而旧的内部实现式写法在发布页上会变成一整堵难读的文字墙，所以整个 CHANGELOG 都已经重写成当前格式，历史发布说明也同步过了。）
2. **每个 bullet 只写一句自然语言：** 说清改了什么，以及这对用户有什么意义。优先从新能力出发，或者从现在被修掉的用户症状出发。
3. **删掉内部细节。** 不写内部文件路径（`src/...`）、内部符号/函数/类名，也不写 benchmark 数字、百分比、节点/边数量。**可以保留：** 语言和框架名称（Go、Spring、NestJS 等）、用户会输入或设置的东西（`rustcodegraph install`、`rustcodegraph_explore`、`RUSTCODEGRAPH_*` 环境变量）、代理/IDE 名称（Codex、Cursor、opencode、Kiro 等），以及当需要署名贡献者时简短写一句 `Thanks @user`。
4. 条目中的 Issue / PR 引用使用编号形式（如 `(#403)`）；GitHub 会在发布说明里自动链接。
5. **不要手动添加 `[X.Y.Z]: https://...` 这种链接引用。** `rustcodegraph prepare-release` 在提升版本时会自动补上（幂等：如果已经存在，重复运行不会再改）。

像 `### New Features` 这种多词标题，在正常发布路径下是安全的：`rustcodegraph prepare-release` 的 **Case A** 会把整个 `[Unreleased]` 内容原样移动到 `[X.Y.Z]`。只有很少使用的 **Case B** 合并路径，才会用一个只匹配单词标题的 `^### (\w+)$` 正则来拆分子节，而它只会在你预先创建了 `[X.Y.Z]` 块时触发；前面的规则已经明确禁止这么做。

### 发布流程（由用户执行）

发布由 **GitHub Actions 的 “Release” workflow**
（`.github/workflows/release.yml`）完成构建与发布。发布准备步骤会使用 Rust 二进制把
`[Unreleased]` 提升成 `[<version>]`，之后 workflow 会构建 cargo-dist 生成的原生
Rust 制品，从提升后的 changelog 区块里提取 GitHub Release notes，
并发布 GitHub Release、Homebrew formula，以及 cargo-dist 的 npm installer
package。现在再手动发布已经是 **错误做法**，因为 artifact 上传和 trusted-publishing
认证都归这个 workflow 管。

**除非用户明确要求，否则 Codex 不会帮你 bump 版本。** 维护者通常自己做这件事，很多时候是直接在 GitHub 网页上编辑 `package.json`。不要把版本升级作为无关工作的顺手提交，也不要在 PR 总结里主动提议 bump 版本。

当维护者**确实** bump 版本时，严格来说唯一必须改的是
`package.json`。workflow 里的 “Sync package-lock.json” 步骤会检测
`package.json` 与 `package-lock.json` 是否不一致，然后运行
`npm install --package-lock-only --ignore-scripts` 来重写 lock file 的
版本字段（顶层和 `packages.""`），并自动提交再推回 `main`，commit message 带 `[skip ci]`。因此，直接在 GitHub 网页上只改一个 `package.json` 文件，就足以触发一套干净的发布流程。（如果维护者本地同时改了两个文件也没问题，这一步会 no-op。）

当 `main` 上的 `package.json` 已经变成目标版本后，在打 release tag / 触发 workflow 之前，先对同一版本运行 Rust 侧的准备命令：

```bash
cargo run --bin rustcodegraph -- prepare-release <X.Y.Z>
```

然后 workflow 会：

1. 构建所有 cargo-dist 的原生 Rust 制品与 checksum。
2. 用 `rustcodegraph extract-release-notes <X.Y.Z>` 生成的内容创建 GitHub Release。
3. 使用 GitHub Actions 认证发布 Homebrew formula 和 npm installer package。

**不要自己运行 `npm publish`、`git push` 或 `git tag`。** 这些都是会作用于共享状态的发布动作。你要做的是改文件，然后把命令交给用户执行。

## 仓库规则

- `0.7.x` 这条线仍处于多代理推广阶段。任何对 `src/installer/` 的改动，尤其是 `targets/`，都必须附带对应测试覆盖和 CHANGELOG 条目，因为安装器回归会悄无声息地影响所有新安装。
- 当你修改 MCP 工具的行为，或修改代理应该如何使用它们时，先改 `src/mcp/server_instructions.rs`。仓库里提交的 `.cursor/rules/rustcodegraph.mdc` 是 dogfooding 配置；如果你在这个仓库里用 Cursor，也一并更新它，但它不会随产品发布。
- RustCodeGraph 提供的是 **代码上下文**，不是产品需求说明。做新功能时，要向用户确认 UX、边界情况和验收标准，图本身不会告诉你这些。
- **当用户引用 issue、PR 评论或外部报告时，在下结论前先把它锚定到日期和版本。** 把评论的 `createdAt` 与以下信息对齐：
  - **最后一个已发布版本**：`grep -m1 '^## \[' CHANGELOG.md` 可以看到文件顶部的最新版本（更旧版本依次往下）。如果评论日期早于最新的 `## [X.Y.Z] - YYYY-MM-DD`，那它反映的是*已发布*状态，说明只存在于 `main` 或某个未合并分支上的工作并不适用。
  - **`main` 上最后一个提交**：`git log --first-parent main -1 --format='%ai %h %s'`。如果评论晚于最后一次发布、却早于某个修复合入 `main`，那它有可能已经在 `main` 上被修好，但还未发布。
  - **当前分支的 tip**：你自己还没合并的工作，显然不可能是评论对象。
  在认同“用户报告的问题还没修”或“某个修复覆盖不完整”之前，必须先区分清楚“已发布”“已合并但未发布”“进行中”这三种状态。用户说“你的修复只覆盖了 X”时，通常指的是*已发布版本*中的不足；而你当前分支上的进行中工作，用户通常根本看不到。
- **给 `README.md` 中引用的每一张图片都加版本标签。** GitHub 会缓存 README 图片（`raw.githubusercontent.com` 有 5 分钟 TTL；第三方图片还会经过长期缓存的 camo 代理），所以原地替换资源文件时，经常会一直看到旧图。每个 README 图片 URL 都要带 `?v=N` 这样的查询参数，并且在同一个提交里，只要图片字节内容变了，就同步把 `N` 加一，例如 `assets/waitlist.svg?v=2`。URL 改变后可以直接绕过缓存，新图会立即生效，而不是等 TTL。
