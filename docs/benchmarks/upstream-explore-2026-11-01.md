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

## 未完成的验收

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
