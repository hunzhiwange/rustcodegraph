<div align="center">

# RustCodeGraph

[English](README.md) · 简体中文

### 为 Claude Code、Cursor、Codex、opencode、Hermes Agent、Gemini、Antigravity 和 Kiro 提供本地语义代码智能

**100% 本地 · Rust 原生 · 面向 AI 编程代理的代码知识图谱**

这个仓库里的 RustCodeGraph 是基于 CodeGraph 修改而来，并围绕当前的 Rust CLI 与 MCP 工作流继续演进。它保留了“让 AI 代理直接查询代码结构，而不是反复 grep/Read 探索文件”的核心思路，但这里的文档、安装方式和产品说明都以当前 `rustcodegraph` 项目为准，不再沿用旧的 CodeGraph 描述。

[英文文档](docs/user/README.md)

</div>

## 这个项目是什么

RustCodeGraph 会在每个项目里建立一个本地 `.rustcodegraph/` 索引，用 tree-sitter 解析源码，把符号、调用关系、导入关系、框架路由和文件结构写入 SQLite。Claude Code、Cursor、Codex CLI、opencode 等 AI 编程代理可以通过 MCP 工具直接查询这个图谱。

换句话说，它让代理少做“到处找文件、反复搜索、再读取源码”的工作，更多地直接拿到结构化上下文：某个函数由谁调用、会调用谁、一次改动可能影响哪些文件，以及一条业务流程如何跨文件、跨框架甚至跨语言串起来。

> RustCodeGraph 是独立的 Rust 重写项目，不是旧 CodeGraph 包的就地升级。如果你之前安装过旧的 CodeGraph，需要单独安装并配置 `rustcodegraph`。

## 快速开始

### 1. 安装 CLI

不需要 Node.js。安装脚本会下载当前系统匹配的 Rust 二进制文件：

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh

# Windows (PowerShell)
irm https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.ps1 | iex
```

如果你已经有 Node.js，也可以使用 npm：

```bash
npm i -g rustcodegraph
```

安装完成后请打开一个新终端，让 `rustcodegraph` 出现在 PATH 中。

### 2. 连接你的 AI 代理

```bash
rustcodegraph install
```

安装器会自动检测并配置 Claude Code、Cursor、Codex CLI、opencode、Hermes Agent、Gemini CLI、Antigravity IDE 和 Kiro，把 RustCodeGraph MCP 服务写入它们的配置。

### 3. 初始化项目

```bash
cd your-project
rustcodegraph init -i
```

`rustcodegraph init -i` 会创建本地 `.rustcodegraph/` 目录，并立即构建完整代码图谱。之后 MCP 服务会自动监听文件变化并同步索引，一般不需要手动运行 `rustcodegraph sync`。

### 4. 卸载

```bash
rustcodegraph uninstall
```

这会移除 RustCodeGraph 写入各个代理的 MCP 配置和说明，但不会删除项目里的 `.rustcodegraph/` 索引。要移除某个项目的索引，请在项目目录运行：

```bash
rustcodegraph uninit
```

## 为什么需要 RustCodeGraph

AI 编程代理理解一个陌生代码库时，通常会用 grep、glob 和 Read 一步步探索文件。这个过程慢、工具调用多，而且容易在大型仓库里消耗大量上下文。

RustCodeGraph 提前把代码库索引成知识图谱，让代理直接查询：

- 某个符号的定义、源码和调用链
- 某个函数的调用者和被调用者
- 一次修改的影响半径
- Web 框架路由到处理函数的映射
- React / 回调 / 跨语言桥接等静态分析容易断开的流程
- 按文件和符号组织好的源码上下文

完整英文 README 中包含 7 个真实开源仓库的基准测试。当前结论是：启用 RustCodeGraph 后，代理通常用更少的工具调用、更少的 token 和更短的时间回答结构性问题。

## 核心功能

| 功能 | 说明 |
|---|---|
| 本地知识图谱 | 符号、文件、调用、导入、继承、路由等信息保存在项目本地 SQLite 中 |
| 语义探索 | `rustcodegraph_explore` 一次返回相关符号源码、关系图和影响范围 |
| 全文搜索 | 基于 SQLite FTS5 快速查找符号和代码 |
| 调用链分析 | 查询 callers、callees 和 impact radius |
| 自动同步 | 使用 FSEvents、inotify、ReadDirectoryChangesW 监听文件变化并增量更新 |
| 框架感知 | 识别 Django、Flask、FastAPI、Express、NestJS、Laravel、Rails、Spring、Gin、Axum、Vapor、React Router、SvelteKit、Vue/Nuxt、Astro 等路由 |
| 跨语言桥接 | 支持 Swift/Objective-C、React Native、Expo Modules、Fabric/Paper 视图等常见跨语言调用关系 |
| 隐私优先 | 代码不离开本机，不需要 API key，不依赖外部索引服务 |

## 支持的代理

`rustcodegraph install` 会自动检测并配置：

- Claude Code
- Cursor
- Codex CLI
- opencode
- Hermes Agent
- Gemini CLI
- Antigravity IDE
- Kiro

## 支持的语言

RustCodeGraph 支持 20 多种语言和文件类型，包括：

- TypeScript / JavaScript
- Python
- Go
- Rust
- Java
- C#
- PHP
- Ruby
- C / C++
- Objective-C
- Swift
- Kotlin
- Scala
- Dart
- Svelte
- Vue
- Astro
- Liquid
- Pascal / Delphi
- Lua / Luau
- R

语言支持会根据文件扩展名自动启用，不需要额外配置。

## 工作原理

```text
源码文件
  -> tree-sitter 解析
  -> 提取符号、调用、导入、继承和路由
  -> 写入 .rustcodegraph/rustcodegraph.db
  -> MCP 工具向 AI 代理提供结构化上下文
```

处理流程分为四层：

1. **提取**：tree-sitter 把源码解析成 AST，语言提取器从中抽取函数、类、方法、变量、导入等节点。
2. **存储**：所有节点、边和文件信息写入本地 SQLite，并启用 FTS5 全文搜索。
3. **解析**：导入、调用、继承和框架约定会被解析成跨文件关系。
4. **查询**：CLI 和 MCP 工具把图谱查询结果格式化为代理可直接使用的上下文。

## 常用 CLI

```bash
rustcodegraph install              # 配置 AI 代理
rustcodegraph uninstall            # 从代理配置中移除 RustCodeGraph
rustcodegraph init -i              # 初始化并索引当前项目
rustcodegraph uninit               # 删除当前项目的 RustCodeGraph 索引
rustcodegraph index                # 全量重建索引
rustcodegraph sync                 # 增量同步
rustcodegraph status               # 查看索引状态
rustcodegraph query UserService    # 搜索符号
rustcodegraph explore "auth login" # 探索相关源码和调用路径
rustcodegraph node UserService     # 查看单个符号或文件
rustcodegraph callers login        # 查看调用者
rustcodegraph callees login        # 查看被调用者
rustcodegraph impact login         # 分析修改影响范围
rustcodegraph affected --stdin     # 根据变更文件推导受影响测试
rustcodegraph upgrade              # 原地升级
```

## MCP 工具

作为 MCP 服务运行时，RustCodeGraph 默认暴露一组面向代理行为优化过的工具：

| 工具 | 用途 |
|---|---|
| `rustcodegraph_explore` | 首选工具。回答“X 如何工作”“X 如何到达 Y”这类结构性问题，返回相关源码、关系和影响范围 |
| `rustcodegraph_node` | 查看某个符号完整源码和调用轨迹，也可以像 Read 一样读取整个文件 |
| `rustcodegraph_search` | 按名称搜索符号 |
| `rustcodegraph_callers` | 找到某个函数或方法的所有调用点，包括回调注册位置 |

在没有 `.rustcodegraph/` 索引的工作区中，MCP 服务会报告自己未激活并隐藏工具；是否索引项目始终由用户决定。

## 配置和隐私

RustCodeGraph 默认零配置。它会自动跳过常见依赖、构建和缓存目录，例如 `node_modules`、`vendor`、`dist`、`build`、`target`、`.venv`、`Pods`、`.next` 等，也会尊重 `.gitignore`。

RustCodeGraph 会收集匿名使用统计，用来判断哪些语言和代理支持最值得改进。它不会上传代码、路径、文件名、符号名、查询内容或 IP 地址。你可以随时关闭：

```bash
rustcodegraph telemetry off
```

也可以设置环境变量：

```bash
RUSTCODEGRAPH_TELEMETRY=0
DO_NOT_TRACK=1
```

详细字段见 [TELEMETRY.md](TELEMETRY.md)。

## 故障排查

**提示 `RustCodeGraph not initialized`**

请先在项目目录运行：

```bash
rustcodegraph init -i
```

**索引很慢**

确认大型依赖或生成目录已经被 `.gitignore` 排除。RustCodeGraph 默认会跳过常见目录，但仓库中特殊的构建产物可能需要你自己加入 `.gitignore`。

**MCP server 连接不上**

代理会自己启动 MCP 服务，通常不需要手动运行 `serve --mcp`。请确认项目已经初始化并索引：

```bash
rustcodegraph status
```

如果配置损坏，可以重新运行：

```bash
rustcodegraph install
```

**仍在使用旧 CodeGraph**

旧 CodeGraph 和 RustCodeGraph 是两个独立项目。请安装 `rustcodegraph`，并确认你的 MCP 配置指向 `rustcodegraph` 而不是旧的 `codegraph` 命令。

## 许可证

MIT

---

<div align="center">

**Made for AI coding agents — Claude Code, Cursor, Codex CLI, opencode, Hermes Agent, Gemini CLI, Antigravity IDE, and Kiro**

[Report Bug](https://github.com/hunzhiwange/rustcodegraph/issues) · [Request Feature](https://github.com/hunzhiwange/rustcodegraph/issues)

</div>
