# 检索控制仓库冻结（任务 032）

记录日期：2026-11-02。上游来源固定为 `15483713763f344ff30fb7df92bcf091c7bd9758`；本轮是证据补充，未修改检索实现。以下 SHA 来自真实 GitHub remote HEAD 查询或副本，后续必须 checkout 指定 SHA，不继续使用浮动 HEAD。

| 控制仓库 | 固定 commit | 固定 explore 查询 | 状态 |
|---|---|---|---|
| expressjs/express | 1c6f1b0fe4000977b9a7b4f34097e4d527dd9824 | `handle render` | 已索引并执行；尚未证明端点连通 |
| excalidraw/excalidraw | 8bcdca4526867433f99fdfcb55a063668db0c9dc | `mutateElement triggerUpdate triggerRender render StaticCanvas renderStaticScene` | 已下载、索引并探测；Flow 仅 StaticCanvas，未通过 |
| django/django | a05e16a7230fe33d3f43f4a3aaed103cebb42efc | `get_response _get_response render` | 仅冻结，未探测 |
| microsoft/vscode | 67710841ad061a9e253214f87bf5415a185ab2ff | `executeCommand _tryExecuteCommand executeCommandHandler` | 仅冻结，未探测 |

这些是本轮新冻结的版本，不能与历史 README 的文件数或成功率混用。未核对查询符号的仓库不计通过；后续须核对实际符号和 Flow 后再批准为有效控制案例。

## 已运行证据

动态领取共享槽位 1：`CARGO_TARGET_DIR` 为仓库绝对路径下的 `target/rust-slot-1`，`CARGO_BUILD_BUILD_DIR` 为其中的 `build`；每阶段结束释放锁，保留缓存。

- `cargo test -p rustcodegraph --test upstream_031_test -- --nocapture`：退出 0，1 passed，0 ignored。
- `cargo fmt --all` 后 `cargo test -p rustcodegraph --test upstream_032_test -- --nocapture`：退出 0，3 passed，0 ignored。测试用原生解析器和真实 SQLite，验证合成连通调用的中间源码与终点函数体，以及单个/多个缺失符号不返回无关函数体。首次运行即通过，因此没有制造红灯。这些不是 Excalidraw 的真实框架验证。
- `cargo run --bin rustcodegraph -- init -i /tmp/rcg-control032/express`，随后 `cargo run --bin rustcodegraph -- agent-eval probe-explore /tmp/rcg-control032/express 'handle render'`：探针退出 0，输出 5376 字符。Flow 仅出现 render，不能证明 handle 到 render 的链路。原始日志在 `/tmp/rcg-control032/express.index.log` 和 `express.probe.log`，为本机临时诊断文件。

初次 init 未加 `-i`，探针返回无结果；随后显式建索引重跑，以上只记重跑结果。不把成功退出当作流程覆盖通过。

## 早期阶段未完成的验收（由下方最终执行结果更新）

Excalidraw 的新版本/基线两分支各至少两次 Sonnet/high A/B 尚未执行；Read/Grep、时长、真实 MCP 使用、逐轮 usage 均没有本轮数据。不能据此声称源码去重或预算分配无回归。还需完成真实 Excalidraw 探针、节点规模与合成边精度检查及其余控制查询的符号核对。

`command -v claude` 未找到；检查常见本机安装目录未发现 Claude 可执行文件。存在配置目录并不证明存在可用认证，未读取或输出凭证。已安装 Codex 的 Sonnet/high 最小请求先遇到 TLS EOF，回退 HTTPS 后明确返回：该 ChatGPT 账号不支持 sonnet 模型。需要外部提供已安装且可认证使用 Sonnet/high 的 Agent 运行环境；不切换其他模型、不自行安装或修改认证。

任务保持已阻塞，报告与测试只保留已经取得的部分证据。

## 第二次恢复验证（2026-11-02，当前引擎 da14247c）

Excalidraw 的 Git shallow/partial clone 本次成功，checkout 与 `git rev-parse HEAD` 确认为表中固定 SHA。无需继续把下载当作阻塞。备用 codeload 下载也成功，实际索引使用 Git checkout。

动态原子领取槽位 1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1`，构建和测试设置 `CARGO_BUILD_BUILD_DIR=$CARGO_TARGET_DIR/build`。以下命令实际执行，阶段后锁均已释放：

```sh
cargo test -p rustcodegraph --test upstream_032_test -- --nocapture
cargo build --bin rustcodegraph
$CARGO_TARGET_DIR/debug/rustcodegraph init -i /tmp/rcg-control032/excalidraw
$CARGO_TARGET_DIR/debug/rustcodegraph agent-eval probe-explore /tmp/rcg-control032/excalidraw 'mutateElement triggerUpdate triggerRender render StaticCanvas renderStaticScene'
```

上述命令退出码均为 0；测试为 3 passed、0 failed、0 ignored，未改 Rust 代码。索引为 738 文件、3159 节点、10798 边。探针的实际 Flow 为：

```text
## Flow (call path among the symbols you queried)
- StaticCanvas
```

SQLite 精确查询六个端点名称，只返回 `StaticCanvas`（function，`packages/excalidraw/components/canvases/StaticCanvas.tsx`）。源码核实 `packages/element/src/mutateElement.ts:40` 和 `packages/excalidraw/renderer/staticScene.ts:658` 确实定义相应导出函数，故这是当前索引覆盖不足的证据，不能把单节点 Flow 解释成完整路径。未在本证据任务扩展提取/合成实现。

重复执行同一 `init -i` 命令退出 0，节点数仍为 3159，未发生节点增长。10798 条边均为 heuristic；抽查前三条 metadata 为 NULL，未取得目标流程合成 wiring 的精度证据。稳定的节点数不能代替覆盖和精度验收。

一次尝试 `index --path /tmp/rcg-control032/excalidraw` 退出 1（该调用未指向目标项目）；随后用上述已验证的 `init -i` 正确重跑，不计首次调用为有效验证。原始日志保存在本机 `/tmp/rcg-control032/{build.log,excalidraw.index.log,excalidraw.probe.log,excalidraw.reindex.log}`。

剩余阻塞有两项：真实控制路径在当前索引中不连通；规定 Sonnet/high 的认证执行环境仍缺失。已向维护者询问可用环境，不改模型或伪造 A/B。其余两个仓库仍仅冻结，未声称通过。

## CLI 接线修复（2026-11-02）

进一步排查发现，早前 `init -i` 调用的是独立的轻量正则索引器，并非库的 AST/解析/合成链。因此早前的 3159 节点结果不能用来评价完整库索引覆盖。CLI 全量索引现改为调用 `CodeGraph::index_all`，保留命令参数与摘要输出。

新增 CLI 箭头函数端点测试先失败（只返回 1 个端点，预期 2 个），修复后通过。`cargo fmt --all` 后执行 `cargo test --test upstream_032_test --test index_command_test --test cli_mcp_parity_test -- --nocapture`：退出 0，分别 4、7、4 passed，合计 15，0 ignored。红灯与绿灯动态领取槽位 2，绝对 CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-2；CARGO_BUILD_BUILD_DIR 为其中 build。日志 cli-red.log、cli-green.log 位于此前临时日志目录，锁已释放。

临时诊断测试直接调用库对固定 Excalidraw 执行完整 index_all（槽位 1，退出 0，1 passed、0 ignored），耗时 239325ms；738 文件，17697 节点，66596 边，无提取错误。诊断测试已移除，避免把本机副本路径写成永久测试依赖。新索引恢复 mutateElement、triggerUpdate 和 renderStaticScene。用新构建再次 probe-explore 退出 0，但没有形成目标完整路径：App.tsx 超出库的原生解析大小阈值，triggerRender 被 fallback 识别为 property，而非带函数体的 method。此处还需要独立修复大文件方法抽取，不能将节点恢复等同于整条流程通过。原始日志 native.log、native.probe.log 在此前临时日志目录。

通过 npm exec 临时取得 Claude CLI，`claude auth status` 退出 0，明确 loggedIn=false、authMethod=none。已解除 CLI 安装缺失，实际 Sonnet 请求仍需账号认证；已提供登录命令供维护者完成。没有更换规定模型，没有伪造 A/B 结果。

## 最终执行结果（2026-11-02）

维护者明确将本任务的强制 Sonnet/Claude 条件改为“当前可用 Codex 模型，两分支相同”。本次固定 `gpt-6.1-sol`、high，不再依赖 Claude CLI 或 Claude 认证。终端显式使用 macOS 已配置的本机 HTTP 代理后，Codex smoke 请求成功。模型变更后的数据不能与历史 Sonnet 结果混算。

真实 MCP 运行又定位到一处接线缺口：stdio 的 explore 原来只使用独立的轻量查询，实际结果只有标题，未经过库的充分性输出与会话去重。现已将 stdio explore/node 接到连接内持久的 ToolHandler，并在连接释放时关闭缓存图。新增端到端 stdio 测试断言返回真实函数体；此前实际 A/B 日志中的空 explore 标题构成缺失行为证据。

修复提交：`4bac4801`（CLI 索引）、`690789dd`（stdio 工具接线）。基线固定 `f8456aa0`。本次比较两种引擎，但两边使用同一份完整库索引的独立副本，以隔离检索差异；它不是索引构建耗时 A/B。所有运行先预热独立 daemon，设置 MCP required=true，并使用完全相同的明确 explore-first 提示。早期未调用 MCP、受审批策略拦截及中途模型容量失败的试跑不计入有效样本；运行副本允许 MCP 工具执行，提示明确禁止编辑文件。

| 引擎/有效轮次 | 秒 | MCP 调用 | 全部工具调用 | 仓库源码 Read / Grep | input / output tokens |
|---|---:|---:|---:|---|---|
| baseline 1 | 172.5 | 26 | 27 | 0 / 0 | 334950 / 4821 |
| baseline 3（补跑） | 150.1 | 32 | 33 | 0 / 0 | 501301 / 4214 |
| new 1 | 140.6 | 29 | 31 | 0 / 0 | 506431 / 3699 |
| new 2 | 173.1 | 31 | 33 | 0 / 0 | 627522 / 4435 |

四次有效运行均退出 0，MCP 调用均无 transport/tool error。baseline 2 在运行中返回模型 at capacity，退出 1，保留日志并补跑为 baseline 3，未换模型。全部工具调用按 item.completed 计数，包含 MCP 和 shell；每轮都有一次读取技能说明的 shell，新版本另有一次索引 status 检查，均不是仓库源码 Read/Grep。完整命令列表与原始 turn.completed usage 保存于相邻 `upstream-explore-2026-11-01-results.json`；cached tokens 是 input 的子集，未重复相加。该 JSON 保留 Codex 原始整用户轮 usage，并不伪称拥有逐次内部 assistant response 的 usage 或美元成本明细。

结论：本任务要求的四仓库冻结与 Excalidraw 两分支各两次有效对照已完成；源码 Read/Grep 未增加。耗时范围重叠（基线 150–172s，新版本 141–173s），只能观察到这四轮的均值略低，不能声称稳定加速。新版本总调用次数与 token 未改善，不能据此宣布全面性能无回归。目标完整 Flow 仍未连通，正常流程仅用少量 explore 的产品目标尚未达到；此处明确记录为后续检索充分性优化依据，而非宣称真实框架全流程覆盖通过。

重复完整索引也已完成：在独立 stability 副本执行新引擎 `init -i`，退出 0，738 文件、17697 节点、66566 边、255928ms。节点数与首次完整索引的 17697 相同，没有节点增长；边数与首次 66596 相差 30，未声称边集合完全稳定。原始索引中合成元数据统计为 jsx-render 329、react-render 42、interface-impl 14；抽查 setUsername/setActiveRoomLink → Collab.render 的 react-render 元数据带 registeredAt，缺少 triggerRender callback 桥接的局限仍如前述。

新增验证（均先 cargo fmt --all，默认 features）：

- `cargo test --test upstream_032_test --test cli_mcp_parity_test --test mcp_initialize_test -- --nocapture`：退出 0，5+4+3=12 passed、0 ignored。
- `cargo test --test mcp_unindexed_test --test mcp_staleness_banner_test --test mcp_tool_allowlist_test --test mcp_roots_test -- --nocapture`：退出 0，7+8+7+5=27 passed、0 ignored。
- 两阶段动态领取槽位 1；`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1`，`CARGO_BUILD_BUILD_DIR=$CARGO_TARGET_DIR/build`，完成后释放锁。日志为 `/tmp/rcg-control032/{stdio-green.log,mcp-boundary.log}`。前述 15 项测试阶段的 upstream_032 当时为 4 项，加入 stdio 后为 5 项，不将重复测试累加为新增用例。

复现脚本：`scripts/agent-eval/run-codex-control.py`，需要 root 目录中放置固定源码+索引的 excalidraw、engine-base、engine-new。显式使用本机代理的实际启动命令：

```sh
HTTPS_PROXY=http://127.0.0.1:7897 HTTP_PROXY=http://127.0.0.1:7897 ALL_PROXY=http://127.0.0.1:7897 python3 scripts/agent-eval/run-codex-control.py /tmp/rcg-control032
# baseline 2 容量失败后，仅补跑该分支：
HTTPS_PROXY=http://127.0.0.1:7897 HTTP_PROXY=http://127.0.0.1:7897 ALL_PROXY=http://127.0.0.1:7897 python3 scripts/agent-eval/run-codex-control.py /tmp/rcg-control032 --iterations 3 --arms base
```

原始 stream-json、stderr、daemon 日志与运行元数据在 `/tmp/rcg-control032/ab-wired-{base,new}-{1,2,3}` 的实际存在目录中。首次四轮脚本为同逻辑的临时版本；仓库版增加选择补跑轮次与拒绝覆盖已有证据。所有实验 daemon 都在运行结束后回收。Express 的既有探针仍不代表完整 Flow 通过，Django/VSCode 本任务仅冻结，未冒充已运行。没有修改 plan.md、版本或发布状态。

## 后续优化：紧凑符号地图（2026-11-02）

逐条审计 new-1 的真实日志，定位到 `node(file=App.tsx, symbolsOnly=true)` 返回 421940 字符。它不是源码，而是 5187 条符号位置，存在重复文件路径及重复同名成员。真实索引中 App::if 出现 481 次、App::return 出现 177 次，说明大文件 fallback 的抽取范围还有噪音；本次不改图和调用边。

本次只优化符号地图序列化：移除标题已有的完整文件名（兼容原生抽取使用 basename 的限定名），按 kind+剩余限定名聚合行号，保留所属类型和全部不同位置。没有新增截断或条目上限。真实冻结副本回放得到 87058 字符，减少 79.4%，输出行从 5190 降到 1421。将新旧输出还原为 `(kind, relative-qualified-name, line)` 集合，均为 5187 项，missing=0、added=0。原始新输出 `/tmp/rcg-control032/outline-new.txt`；旧输出来自 ab-wired-new-1/run.jsonl 的 symbolsOnly 调用。

失败测试 `symbols_only_groups_repeated_names_without_losing_locations_or_owners` 在修改前退出 101；新测试验证两个同名定义位置都保留，One::run 与 Two::run 不合并，文件标题保留且无函数体。首次实现暴露原生抽取使用 basename 而非相对路径的差异，修正后验证通过。

`cargo fmt --all` 后执行 `cargo test --test node_file_view_test --test upstream_032_test --test cli_mcp_parity_test -- --nocapture`，退出 0，12+5+4=21 passed、0 ignored。阶段动态领取槽位 1，绝对 CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1，CARGO_BUILD_BUILD_DIR 为其中 build；锁均释放。日志 `/tmp/rcg-control032/{outline-red.log,outline-green.log}`。真实回放命令为该槽位 debug/rustcodegraph 的 `node --file packages/excalidraw/components/App.tsx --symbols-only -p /tmp/rcg-control032/excalidraw`，退出 0。

这是单次工具输出的确定性缩减证据，没有重跑 Agent A/B，不能据此声称整轮时延、工具次数或总 token 已改善。后续高价值方向仍是大文件 fallback 的作用域和多行方法抽取，需要确保 callback→react-render→jsx 的整条路径一起验证，不能仅修一个桥接后宣布充分性达标。
