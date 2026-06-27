# 匿名使用遥测

状态：已实施 - 摄取工作人员 (`telemetry-worker/`)、客户端 (`src/telemetry/`)、
`rustcodegraph telemetry` CLI、MCP + 安装程序接线、`TELEMETRY.md`。待定：工作人员部署
+ DNS，释放。
范围：公共 RustCodeGraph 引擎（CLI + MCP 服务器 + 安装程序）

RustCodeGraph 是一个本地优先的工具，其核心是“你的代码永远不会离开你的机器”。
遥测技术的设计必须确保句子真实且可证明：一个简短的、可审计的列表
匿名计数器，逐个字段记录，易于关闭，并且不可能增长
悄悄。该文件是合同； `TELEMETRY.md`（存储库根，面向用户）重申它并且
实现绝不能收集此处未列出的任何内容。

## 目标

汇总且匿名回答：

- 有多少台机器主动使用 RustCodeGraph（每天/每周），情况有何变化？
- 哪些代理推动使用（Claude Code、Cursor、Codex、opencode 等）——通过 MCP `clientInfo`。
- 人们选择哪种安装目标：本地安装还是全局安装、全新安装还是升级安装。
- 使用哪些 MCP 工具和 CLI 命令、使用频率以及出错的频率。
- 人们索引哪些语言（根据实际使用情况优先考虑提取器/框架工作）。
- 版本采用速度、操作系统/架构/节点混合、本机与 wasm SQLite 后端共享。

## 未进球/从未进球

- **永远没有源代码。**没有文件路径、文件名、存储库名称、符号名称、查询
字符串、搜索词或从索引项目的内容派生的任何内容。
- 没有 IP 地址（在边缘被剥离；在后端也禁用存储）。
- 无硬件指纹识别 - 机器 ID 是随机 UUID，不源自任何内容。
- 没有每次击键/每次调用事件流 - 使用情况在本地聚合到每日汇总中
在发送任何内容之前。
- 没有来自 `rustcodegraph-pro` 分叉的遥测（请参阅下面的“rustcodegraph-pro 规则”）。

## 原则

1. **架构是允许列表。**客户端仅发送以下事件；摄取工人
根据相同的允许列表进行验证并删除其他任何内容。添加一个字段 = PR
一起编辑此文档 + `TELEMETRY.md` + Worker 白名单。
2. **遥测可能永远不会让用户付出任何代价**：MCP 工具调用的零附加延迟
热路径（存储库的核心不变式），零新的 npm 依赖项（全局 `fetch`，节点 ≥18），
标准输出上的零字节（stdio 是 MCP 协议通道）、零重试、零错误噪声。
每一种故障模式都是沉默的。
3. **关闭就是关闭。** 禁用时，没有进程打开到遥测端点的套接字 - 不
甚至“选择退出”ping。
4. **第一方端点。** 客户端仅与 `telemetry.getcodegraph.com` 通信。网址
烘焙到已发布的 npm 版本中并永远发布在那里，因此域名必须是我们的；这
其背后的后端可以在不发布客户端的情况下进行更改。

## 活动

每批次的公共信封（每个进程计算一次）：

| 场地 | 例子 | 笔记 |
|---|---|---|
| `machine_id` | `b3a8…` (UUIDv4) | 随机，首次运行时生成，存储在全局配置中 |
| `rustcodegraph_version` | `0.9.12` | 来自包元数据 |
| `os` / `arch` | `darwin` / `arm64` | `process.platform` / `process.arch` |
| `node_major` | `22` | 仅专业 |
| `ci` | `false` | `CI` 环境变量存在 |
| `schema_version` | `1` | 架构更改时发生碰撞 |

事件类型：

- **`install`** — 每个安装程序运行一个。道具：`targets`（例如 `["claude","cursor"]`），
`scope` (`local`/`global`)、`kind` (`fresh`/`upgrade`/`reinstall`)、`sqlite_backend`
(`native`/`wasm`)。
- **`index`** — 每个完整索引一个（`init`/`index`，而不是每个 `sync`）。道具：`languages`
（仅名称，例如 `["typescript","go"]`）、`file_count_bucket`（`<100`、`100-1k`、`1k-10k`、
`10k+`）、`duration_bucket`（`<10s`、`10-60s`、`1-5m`、`5m+`）、`sqlite_backend`。
- **`usage_rollup`** — 主力。每台机器每 `(day, kind, name)` 一个事件，
本地聚合。道具：`kind` (`mcp_tool`/`cli_command`)、`name`
（例如 `rustcodegraph_explore`、`affected`）、`count`、`error_count`，对于 MCP：
`initialize` 握手中的 `client_name`/`client_version` (`src/mcp/session.ts`
`case 'initialize'` — 要添加的管道；目前未读）。
- **`uninstall`** — 每次 `uninstall`/`uninit` 运行一个（搅动信号）。道具：`targets`。

数量数学：汇总平均每月事件 ≈ 活跃机器 × 活跃天数 × 不同数量
使用的工具（个位数）——PostHog 免费套餐（1M 事件/月）涵盖数十个
数千月活跃用户。设计上没有每次调用事件。

事件作为 PostHog **匿名事件** (`$process_person_profile: false`) 发送：
更便宜，没有个人资料，独特的机器计数仍然适用于 `distinct_id` =
`machine_id`。仅当保留工具需要配置文件时才重新访问。

## 同意和控制

解决顺序（第一场比赛获胜）：

1. `DO_NOT_TRACK=1`（社区标准 — 始终受到尊重）→ 关闭
2. `RUSTCODEGRAPH_TELEMETRY=0|1` → 强制关闭/打开该进程
3. 全局配置 `~/.rustcodegraph/telemetry.json` → 存储用户选择
4. 默认值：**开启**，由下面的首次运行通知控制

表面：

- **安装程序（交互式）：**现有提示流程中可见的咔哒声切换 -
“共享匿名使用数据？（无代码、路径或名称 - 请参阅 TELEMETRY.md）” - 默认
是的。选择仍然是 `consent_source: "installer"`。重新运行/升级尊重
已存储选择，请勿再次询问。
- **无头路径**（`npx rustcodegraph init`，MCP 服务器 - 无 TTY，从不提示）：正确
在**第一次实际发送**之前（仅记录本地缓冲区并保持沉默 - 所以
安装程序的显式切换始终先于任何通知），打印一行以
**stderr** 并记录 `first_run_notice_shown`：
`RustCodeGraph collects anonymous usage stats (no code or paths) — "rustcodegraph telemetry off" or RUSTCODEGRAPH_TELEMETRY=0 disables. Details: TELEMETRY.md`
- **CLI:** `rustcodegraph telemetry status|on|off`（状态打印机器 ID、当前
状态，以及决定它的因素）。删除 `~/.rustcodegraph/telemetry.json` 会重置所有内容，
包括机器 ID。

`~/.rustcodegraph/telemetry.json`：

```json
{
  "enabled": true,
  "machine_id": "uuid-v4",
  "consent_source": "installer | default-notice | cli",
  "first_run_notice_shown": true,
  "updated_at": "2026-06-12T00:00:00Z"
}
```

（`~/.rustcodegraph/` 是新的 - 今天没有任何全局存在。如果用户曾经通过文件名共存
索引 `$HOME` 本身，因为每个项目的数据都位于 `<project>/.rustcodegraph/` 中，并且固定
其他文件名。）

## 客户端架构

新模块`src/telemetry/`（单个小模块，无依赖）：

- **内存中的计数器** — 记录工具调用/CLI 命令是内存中的增量。
热路径上没有任何内容接触磁盘或网络。 MCP 工具处理程序调用
`telemetry.count('mcp_tool', name, ok)` 并继续前进。
- **缓冲区** — 计数器持续（去抖、异步）至 `~/.rustcodegraph/telemetry-queue.jsonl`。
硬上限~256 KB；溢出时丢弃最旧的行。缓冲区损坏 → 截断，绝不抛出。
- **Flush** — 许多 CLI 操作通过 `process.exit()` 结束，其中 `beforeExit` 永远不会触发
异步发送死亡，所以设计是：`process.on('exit')` 上的一个微小的**同步追加**
保留内存中的增量（`process.exit` 幸存），并且实际的网络发送发生
伺机而动——在长时间运行的命令开始时（`init`/`index`/`sync`/
`uninit`/`upgrade`)，在长寿命 MCP 服务器/守护程序中的未引用间隔上，以及
waited-with-cap 在 `install`/`init`/`index`/`uninit` 末尾，其中第二个是
无形的。将 POST 完成日汇总 + 生命周期事件发送至
`https://telemetry.getcodegraph.com/v1/events` 与 `AbortSignal.timeout(1500)`，
即发即忘：任何响应（或无响应）都是最终的——无需重试，也不会出现错误。这
队列由原子重命名声明，因此并发进程不能双重发送（崩溃
发件人的声明在一小时后合并回来）。 `RUSTCODEGRAPH_TELEMETRY_DEBUG=1` 回声
有效负载到 stderr 进行开发。
- **离线/气隙：**刷新失败，无提示，缓冲区保持在上限内，稳定状态为
有界文件和零噪声。

## 摄取端点 (Cloudflare Worker)

`telemetry.getcodegraph.com` → 居住在这个仓库中 `telemetry-worker/` 的小型 Rust/Wasm Worker —
故意公开，以便任何人都可以准确审核端点存储的内容。它无处可寄
使用 npm 包（被 `files` 白名单排除）：

- `POST /v1/events`：根据事件/属性白名单进行验证（删除未知事件，
剥离未知的属性），强制执行合理的大小，**永远不要转发或记录客户端 IP**
（删除 `CF-Connecting-IP`），轻按 `machine_id` 速率限制，这样滥用就不会烧毁
摄取上限，使用来自 a 的项目密钥转发到 `https://us.i.posthog.com/batch/`
工人秘密。接受时响应 `204`（包括被白名单删除的事件）
对于格式错误/过大/速率受限的请求，诚实的 `4xx` — 客户端对待
每个响应都是最终的，并且不会重试。
- 今天的后端：PostHog Cloud US，免费计划，启用“丢弃客户端 IP”，禁用 GeoIP，
自动捕获/重播/热图/网络生命体全部关闭。 Worker 是接缝：交换
后端稍后是 Worker 更改，而不是客户端版本。

## rustcodegraph-pro 规则（不要在上游合并中丢失此规则）

私人 `rustcodegraph-pro` 叉子在客户集装箱内运输，其保证是
“没有任何东西离开盒子”——包括遥测。在分叉中，遥测必须是 **default-off
并且安装程序无法启用**（编译时常量或剥离模块），并且
容器设置 `RUSTCODEGRAPH_TELEMETRY=0` 作为皮带和支架。这条规则存在于 fork 中
CLAUDE.md 并且必须在每次上游合并中生存。

## 推出

1. 该文档+ repo-root `TELEMETRY.md`（面向用户的逐字段列表）+ README 部分。
2. Worker + DNS 首先上线（因此第一个发货客户端永远不会出现 404 错误），PostHog 仪表板：
每周活跃机器、按目标安装、按工具 × 客户端使用、版本采用、
索引的语言。
3. 客户端模块 + 配置 + `rustcodegraph telemetry` 子命令 + MCP `clientInfo` 管道。
4. 安装程序切换 + 首次运行通知。 `[Unreleased]` 下的变更日志条目宣布
遥测、默认设置和每个关闭开关。发布。

测试（没有数据库模拟，根据存储库约定；在 `globalThis.fetch` 处模拟获取）：
同意优先级 (env > config > default)，关闭 ⇒ 零获取调用，汇总聚合
跨天，缓冲区上限 + 损坏缓冲区恢复，MCP 传输下的无标准输出不变，
刷新中止遵守超时，安装程​​序切换仍然存在+重新运行不会重新询问
（每个房屋规则为 `__tests__/installer-targets.test.ts`）。

## 开放式问题

- 准确的安装程序副本/通知措辞 - 维护人员在发布前致电。
- `uninstall` 事件：保留还是删除？ （诚​​实的流失信号与“退出”光学器件。）
- CI 事件被保留（标记为 `ci: true`），因为 engine-in-CI 是真正的使用模式 — 重温一下
如果它曾经主导过音量的话。
