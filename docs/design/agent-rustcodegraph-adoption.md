# 让代理实际使用 rustcodegraph（不是阅读）——设计说明和移交

> 新会议的工作文档。需要破解的两个问题：
> **(P1)** 代理在实施过程中仍然会使用 `Read`/`grep` 而不是 rustcodegraph；
> **(P2)** 启动时，当代理第一次启动时，rustcodegraph MCP 服务器可以是 `pending`，因此代理运行时根本不使用 rustcodegraph。
>
> 首先阅读 `rustcodegraph/CLAUDE.md` →“检索性能和动态调度覆盖率”——这是这些想法必须尊重的原则。

---

## 上下文——已经发布的内容（所以你不用重复）

- **#733 (`7175dc4`)** — 重新构建了面向代理的转向（`src/mcp/server_instructions.rs` + `src/mcp/tools.rs` 中的 `rustcodegraph_node`/`rustcodegraph_explore` 描述）以涵盖*实现*，而不仅仅是问答；并添加了**文件视图模式**：`rustcodegraph_node` 现在接受裸 `file`（无 `symbol`）→ 返回该文件的符号映射+其依赖项（爆炸半径）+逐字正文（`includeCode`）。
- **干净的 A/B 结果**（新构建与基线构建，两者都与 rustcodegraph 连接，相同的完全实现的任务 - `kindExclude` 添加到 `rustcodegraph_search`）：
  - **基线：** 0 次 rustcodegraph 调用，8 次读取（代理 *忽略* 可用的 rustcodegraph）。
  - **新：** 2 次 `rustcodegraph_explore` 调用，5 次读取。
  - 因此，重构 *确实* 移动了工具选择 - 但代理使用了 `rustcodegraph_explore`，**从不使用文件视图**，并且仍然读取 5×。 n=1/臂。
- **评估线束修复**（`#735`）：嵌套附加是一个*启动延迟*问题，而不是硬块。 `scripts/agent-eval/ab-new-vs-baseline.sh` 现在预热守护进程 + 跳过重新执行；使用它（非嵌套运行以获得最干净的结果）。

**教义约束（来自 CLAUDE.md — 不要重新提出）：**
- *使工具适应代理。*更改工具描述/`server_instructions.rs` 是**低显着性**并且之前已经*回归*挂钟。仅靠措辞并不能可靠地改变工具的选择。
- *新工具比扩展现有工具更糟糕*（代理甚至没有选择 `trace`；`context` 已被删除）。
- 历史上真正的杠杆：**覆盖率**（更多的流量静态连接 → `explore` 表面它们）和**充分性**（输出足够完整以至于代理*停止*读取）。
- 优化目标是**挂钟+工具调用计数+读取=0**，而不是令牌成本（副作用是成本较低）。

---

## P1 - 代理在实施过程中未充分使用 rustcodegraph

### 状态 — 2026-06-08（通过读取奇偶校验解决，而不是钩子）

**修复：使 `rustcodegraph_node` 读取文件*与读取工具*完全相同，仅
更快——所以代理自然而然地伸手去拿它。不强迫。** 主人的掌控
确定了方向：*“rustcodegraph 应该能够像 Read 一样读取
工具……让它和阅读一样好。读起来又慢又老；查询索引速度很快。
你一直偏离使用 rustcodegraph 而不是寻求修复。”*

**完成 — `src/mcp/tools.rs` 中的文件视图处理现已完全读取奇偶校验：**
- 没有 `symbol` 的 `file` 返回文件的当前源编号
**逐字节读取的方式 — `<n>\t<line>`，无填充，尾随空
行保持**（通过读取同一文件并进行比较来验证）。唯一的
另外还有一个**单行爆炸半径头** (`used by N files: …`)。
- **`offset` / `limit` 的意思正是它们在 Read 上所做的事情**（从 1 开始；最大
线路；默认整个文件上限为 2000 行（如 Read）。大文件分页
老实说（`(lines X–Y of N — pass offset/limit…)`），从来没有15k `truncateOutput` 砍。
- 内容是**默认**（不需要 `includeCode`）； `symbolsOnly: true` 返回
而是廉价的结构图。安全保留：`yaml`/`properties`
按键总结，从未被转储（#383）；通过 `validatePathWithinRoot` 读取（#527）。
- 测试：`tests/node_file_view_test.rs`（包括严格格式奇偶校验
`^1000\t  const v998 = 998;` 和无填充的 `^1\timport …`）。全套房绿色
（1270）。描述 / `server_instructions.rs` / CHANGELOG 重新构造：“读取
使用 rustcodegraph_node 而不是 Read 的源文件 — 相同的字节，更快。”

** 钩子（想法 1）——A/B'd 并被拒绝。请勿发货。** 仅作为评估保留
工件（`scripts/agent-eval/redirect-read-hook.sh` + `ab-hook.sh`）。
- 清理 A/B（2 次运行/臂，devpit“添加 `dp ping`，构建它”；两个臂都附有 rustcodegraph）：
  - **nohook：** 0 rustcodegraph 调用，1 次读取，**5-7 次工具调用，6-8 轮，55-77 秒。**（重现 P1：代理忽略 rustcodegraph — 但读取一次并编辑在这里*有效*。）
  - **钩子（拒绝重定向）：** 0 *成功*读取+ 1个文件视图调用（奇偶校验有效，编辑编译），但是** 8-9个工具调用，9-10转，200-239秒**，并且代理**与拒绝作斗争** - `ToolSearch`找到工具，反身重新读取（拒绝），然后** `Bash python3`读取块周围的文件。**
  - 结论：一揽子拒绝读取**在简单编辑**上回归目标指标（~2×工具调用，更多回合），并且代理围绕它进行路由。强迫是错误的杠杆；让这个工具真正比 Read 更好是正确的选择。
- 如果重新考虑路由：不是毯子钩。要么是窄触发器（大
仅文件/N 次读取后）**在读取繁重的多文件任务上使用干净的 A/B**
（钩子的最佳情况，未经测试），或者只是保持扩大覆盖范围+充分性。

---

**症状：** 即使附加了 rustcodegraph + 新的转向，代理也会在执行过程中反射性地 `Read`s/`grep`s，并且永远不会访问文件视图。描述无法解决此问题（低显着墙）。

### 想法，按预期杠杆排名

1. **PreToolUse(Read/Grep) 重定向到 rustcodegraph** — *最高杠杆；真正改变行为的唯一渠道。*
   - Claude Code **hooks** 可以拦截工具调用并注入上下文或阻止它 - 与描述不同，这“不是”低显着性。我们已经有 `scripts/agent-eval/block-read-hook.sh`，并可在运行时生成临时 settings 把它接入（用于在评估中强制 Read=0）。
   - 发送 **推荐（选择加入）钩子**：在 *索引* 的路径的 `Read` （或 `Grep`）上，注入“此文件已索引 - `rustcodegraph_node {file}` 返回它 + 它的爆炸半径以减少标记；将其输出视为已读。”软推动（不要硬阻止，否则它会让用户在配置/文档 rustcodegraph 不索引时感到沮丧）。
   - 安装程序 (`src/installer/targets/claude.rs`) 可以提供添加此挂钩（选择加入，如自动允许权限）。
   - **使用 `ab-new-vs-baseline.sh` 验证**（读取计数，带钩子与不带钩子）。这是最有可能取得进展的实验。
   - 开放问题：如何知道路径是从钩子内部索引的（查询 `rustcodegraph files`/`status`，或针对 `.rustcodegraph` 的快速本地检查）；避免非索引文件上的噪音；每种语言的误报。

2. **充分性：使文件视图成为明显的读取替换，以便代理*想要*它。**
   - A/B 显示代理从未将 `file` 传递给 `rustcodegraph_node`。为什么？它不认为“读取此文件”→“rustcodegraph_node file=X”。调查：对于代理的下一步（`Edit`），文件视图的值（符号+依赖项+主体）实际上*比读取*更好吗？它返回尸体——但是它是否自信地向 `Edit` 返回足够的周围环境？如果没有，代理无论如何都会读取。
   - 考虑一下：当代理*执行*读取索引文件时，有没有办法使 rustcodegraph 的先前 `explore`/`node` 输出*已经*满足其需要？ （即修复上游充足性，而不是读取本身。）

3. **覆盖范围 - 持久的杠杆。** 每个静态连接的流都是代理不会读取并重建的流。继续缩小动态调度差距 (`src/resolution/`)。少说“停止阅读”，多说“永远不需要”。

4. **命名/可供性实验（低置信度，便宜）。** 文件视图隐藏在 `rustcodegraph_node` 内。一个专门的、明显命名的可供性可能会被更多地选择——*但是*“新工具的情况更糟”，所以这可能会失败。如果尝试过，A/B；不要假设。

**推荐：**原型 **想法 1（读取重定向挂钩）** 和 A/B 它。这是真正有机会改变行为的唯一杠杆。其他一切都是增量的。

---

## P2 — 代理在没有 rustcodegraph 的情况下运行，因为服务器在启动时为 `pending`

**症状：** 当代理第一次启动时 `serve --mcp` 尚未准备好（主机标记 MCP 服务器 `status:"pending"` / 0 工具），因此代理启动 Read/grep 并且从不使用 rustcodegraph。我们在嵌套评估中看到了这一点（大约 2-3 秒启动 vs 代理的第 1 回合）； **真实用户使用较温和的版本** - 会话的第一个查询可能没有 rustcodegraph。

### 根本原因
在 Rust 运行时之前，`serve --mcp` 执行了 Node/V8 标志重新执行，然后在工具可用之前生成/绑定了一个分离的守护进程。 Rust 运行时退出了 Node/WASM 重新执行；剩下的启动风险是负载下的守护进程生成/绑定延迟。预热守护进程仍然会消除评估中的绑定延迟，但真正的用户不需要手动执行此操作。

### 想法，排名

1. **RUSTCODEGRAPH-SIDE — 立即公开静态工具列表，与守护进程分离。 *最大的可发货胜利；帮助每个用户。***
   - 假设：主机将 rustcodegraph 标记为 `pending`，因为 `tools/list`（工具暴露）等待守护进程连接。本地握手路径位于 `src/mcp/proxy.rs`。 **调查：`serve --mcp` 是否在本地立即回答 `tools/list`，还是将其转发到仍在连接的守护进程？** 如果是后者，请将其解耦：在客户端请求时通告静态工具，标记已连接，并在后台解析守护进程以进行实际的工具调用。
   - 验证：`printf '<initialize>\n<initialized>\n<tools/list>\n' | target/release/rustcodegraph serve --mcp --path <repo>` 并计算 `tools/list` 响应的时间，守护进程模式与进程内。进程内应答时间约为 165 毫秒；守护进程模式是嫌疑人。
   - 如果这个落地，`pending`-at-startup 基本上会消失，而无需任何主机更改。

2. **由 RUST 退休完成 - MCP 服务路径上没有节点/WASM 重新执行。** 让启动工作专注于守护进程绑定和静态 `tools/list` 暴露。

3. **RUSTCODEGRAPH-SIDE - 一个 SessionStart 钩子，用于预热守护进程。** 发送一个选择加入的 Claude Code `SessionStart` 钩子（安装程序添加），该钩子在会话启动时生成/加热项目的守护进程，因此它在第一个查询之前绑定。如果 (1) 很难，则进行缓解。

4. **主机端 — “等待/重试挂起” — 这是您所问的问题，但这是 Claude Code（MCP 客户端）行为，而不是 rustcodegraph 需要修复的行为。** rustcodegraph 无法使代理重试。选项：(a) 使用 Anthropic 作为 MCP 客户端改进来提高它（在配置的 MCP 服务器完成连接之前不要让代理的第一回合继续，或者重试 `pending` 服务器）； (b) 注意 `MCP_TIMEOUT` 存在，但在这里**没有**帮助，因为问题是*工具曝光计时*，而不是连接超时。将其作为请求，并依靠 (1)-(3) 来控制我们的控制。

**建议：**追逐**想法1**（将`tools/list`与守护进程解耦）。正是这个修复使 rustcodegraph 立即为每个人“连接”。将 **idea 3**（预热 SessionStart 挂钩）作为一种廉价的并行缓解措施。提交主机端请求 (4) 但不依赖它。

---

## 关键文件/指针

- **转向/工具：** `src/mcp/server_instructions.rs`（`initialize` 指令 - 单一事实来源），`src/mcp/tools.rs`（工具描述 + 处理程序和 `get_static_tools`）。
- **启动/守护进程/代理：** `src/mcp/proxy.rs`、`src/mcp/index.rs`、`src/mcp/daemon.rs` 和 `src/mcp/ppid_watchdog.rs`。
- **挂钩（现有）：** `scripts/agent-eval/block-read-hook.sh`（通过运行时生成的临时 settings 接入；评估的 force-Read-0 挂钩，也是 P1 重定向挂钩的基础）。
- **安装程序（在哪里添加推荐的钩子）：** `src/installer/targets/claude.rs`。
- **评估工具：** `scripts/agent-eval/ab-new-vs-baseline.sh`（新与基线，预热烘烤），`run-all.sh`（有与无），`rustcodegraph agent-eval parse-run`（按类型计算工具计数；`rustcodegraph tools exposed: 0` + 0 rustcodegraph 调用=无运行）。
- **原则：** `CLAUDE.md` →“检索性能和动态调度覆盖范围”+“验证方法”下的代理评估注释。

## 如何验证这里的任何内容
- **P1（读取位移）：** `bash scripts/agent-eval/ab-new-vs-baseline.sh <indexed-repo> "<implementation task>" [baseline-ref]` — 比较 `Read` 与 `mcp__rustcodegraph__*` 计数。 ≥2 次运行/臂（n=1 是有噪音的）。非嵌套运行以获得最干净的结果。使用*真正的新*功能任务（验证它尚不存在 - 第一次 A/B 尝试浪费了在已实现的 `--quiet` 上的运行）。
- **P2（启动）：**时间 `tools/list` 从 `serve --mcp`（上图）；并计算 `init` 显示 `connected` + 工具 > 0 的冷启动运行。不要信任单个 `pending` 初始化快照 - 通过代理是否实际调用 rustcodegraph 进行确认。

## 需要记住的限制/陷阱
- 描述/说明的重要性较低 - **A/B 每项行为主张**，不要凭信心传递措辞。
- 新工具<扩展现有工具。
- 即使服务器随后连接，主机的 `init` 快照也可以显示 `pending` — 根据实际使用情况判断。
- 除非预热，否则不要运行嵌套“干净”数字的评估；即便如此，真正的终端还是更好。

## 新会话的建议开始顺序
1. **P2想法1**——验证`serve --mcp`是否本地/即时回答`tools/list`；如果不是，请将其与守护进程分离。 （价值最高、可交付、帮助所有用户，无需行为猜测。）
2. **P1 想法 1** — PreToolUse(Read) 重定向挂钩原型； A/B 它。 （最高价值的行为杠杆。）
3. 发送 P2 SessionStart 预热挂钩作为缓解措施；提交主机端等待/重试请求。
