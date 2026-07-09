<div align="center">

# RustCodeGraph

[English](README.md) · 简体中文

面向 Claude Code、Cursor、Codex CLI、opencode、Hermes Agent、Gemini、Antigravity 和 Kiro 的本地语义代码智能。

**本地优先 · Rust 原生 · 支持 MCP**

[完整文档](docs/user/README.md) · [反馈问题](https://github.com/hunzhiwange/rustcodegraph/issues)

</div>

## 这是什么

RustCodeGraph 会把项目索引成本地 `.rustcodegraph/` 知识图谱。它用 tree-sitter 解析源码，把符号、文件、调用、导入、框架路由和跨文件关系写入 SQLite，然后通过 CLI 和 MCP 工具提供给 AI 编程代理。

它的目标很简单：让代理少做反复 grep、glob、Read 文件的探索工作，直接查询结构化上下文：

- 符号在哪里定义？
- 谁调用了这个函数？
- 这个函数又调用了什么？
- 修改某个符号可能影响哪里？
- 一个请求、回调、渲染链路或跨语言桥接是怎样串起来的？

RustCodeGraph 是从 TypeScript 版本 CodeGraph 移植而来的 Rust 实现，并围绕原生 CLI、本地 SQLite 索引和 MCP 工作流重新整理。

## 快速开始

### 1. 安装

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh

# Homebrew (macOS / Linux)
brew install hunzhiwange/tap/rustcodegraph

# Windows (PowerShell)
irm https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.ps1 | iex

# 如果你已经使用 Node.js
npm i -g rustcodegraph
```

安装后请打开一个新终端，让 `rustcodegraph` 出现在 `PATH` 中。

### 2. 连接代理

```bash
rustcodegraph install
```

安装器会自动检测支持的代理，并写入启动 RustCodeGraph MCP 服务所需的配置。

### 3. 索引项目

```bash
cd your-project
rustcodegraph init -i
```

这会创建 `.rustcodegraph/`，构建首次索引，并在 MCP 服务运行时自动同步后续文件变化。

### 4. 卸载

```bash
rustcodegraph uninstall  # 移除代理配置
rustcodegraph uninit     # 删除当前项目索引
```

## 核心功能

| 功能 | 说明 |
|---|---|
| 本地知识图谱 | 代码结构保存在项目本地 SQLite 中，不依赖外部索引服务。 |
| 面向代理的上下文 | `rustcodegraph_explore` 一次返回相关源码、关系和流程路径。 |
| 搜索与导航 | 通过 CLI 或 MCP 查询符号、调用者、被调用者和影响范围。 |
| 自动同步 | 原生文件监听器会在编辑后更新图谱。 |
| 框架感知 | 把路由、组件、框架约定和桥接边界连接到实际处理它们的代码。见[支持的框架和桥接](#支持的框架和桥接)。 |
| 跨语言桥接 | 连接 Swift/Objective-C、React Native、Expo Modules 和原生视图等常见跨语言流程。 |
| 隐私优先 | 代码、路径、文件名、符号名和查询内容都留在本机。 |

## 常用 CLI

```bash
rustcodegraph install              # 配置支持的代理
rustcodegraph uninstall            # 从代理配置中移除 RustCodeGraph
rustcodegraph init -i              # 初始化并索引当前项目
rustcodegraph uninit               # 删除当前项目索引
rustcodegraph index                # 全量重建索引
rustcodegraph sync                 # 增量同步
rustcodegraph status               # 查看索引状态
rustcodegraph query UserService    # 搜索符号
rustcodegraph explore "auth login" # 返回相关源码和流程路径
rustcodegraph node UserService     # 查看单个符号或文件
rustcodegraph callers login        # 查看调用点
rustcodegraph callees login        # 查看被调用者
rustcodegraph impact login         # 查看影响范围
rustcodegraph affected --stdin     # 根据变更文件推导受影响测试
rustcodegraph upgrade              # 更新已安装版本
```

## MCP 工具

RustCodeGraph 默认暴露一组为编程代理优化过的 MCP 工具：

| 工具 | 用途 |
|---|---|
| `rustcodegraph_explore` | 首选工具，用于回答“X 如何工作”“X 如何到达 Y”这类结构性问题。 |
| `rustcodegraph_node` | 查看某个符号、重载集合或文件的完整源码，并附带调用上下文。 |
| `rustcodegraph_search` | 按名称搜索符号。 |
| `rustcodegraph_callers` | 查看所有已知调用点，包括回调注册位置。 |

如果当前工作区没有 `.rustcodegraph/` 索引，MCP 服务会报告自己未激活并隐藏工具；是否索引项目始终由用户决定。

## 支持的代理

`rustcodegraph install` 可以配置：

- Claude Code
- Cursor
- Codex CLI
- opencode
- Hermes Agent
- Gemini CLI
- Antigravity IDE
- Kiro

## 支持的语言

**源码语言：** TypeScript/TSX、JavaScript/JSX、Python、Go、Rust、Java、C、C++、C#、PHP、Ruby、Swift、Kotlin、Dart、Pascal/Delphi、Scala、Lua、Luau、Objective-C 和 R。

**组件、模板和配置格式：** Razor/Blazor、Svelte、Vue、Astro、Liquid、YAML、Twig、XML 和 Java `.properties`。

语言支持会根据文件扩展名自动启用。RustCodeGraph 默认跳过常见依赖、构建和缓存目录，尊重 `.gitignore`，并忽略超过 1 MB 的文件。

## 支持的框架和桥接

**后端和 Web 路由：** Django、Flask、FastAPI、Express、NestJS、Laravel、Drupal、Rails、Spring、Play Framework、Gin、chi、gorilla/mux、Axum、actix、Rocket、ASP.NET 和 Vapor。

**前端路由和组件：** React、React Router、Next.js、Svelte/SvelteKit、Vue/Vue Router/Nuxt 和 Astro。

**原生、移动端和跨语言流程：** SwiftUI、UIKit、Swift/Objective-C 桥接、React Native legacy bridge、React Native TurboModules、React Native native events、Expo Modules 和 Fabric/Paper native views。

**工作区约定：** Cargo workspaces、TypeScript path aliases、SvelteKit `$lib` 和 Nuxt/Vue auto imports。

## 故障排查

**`RustCodeGraph not initialized`**
在项目目录运行 `rustcodegraph init -i`。

**索引很慢**
确认大型生成目录或依赖目录已经被 `.gitignore` 排除。

**MCP server 连接不上**
代理会自己启动服务。先运行 `rustcodegraph status` 检查项目状态；如果配置可能过期，重新运行 `rustcodegraph install`。

**从 TypeScript 版本 CodeGraph 迁移**
请安装 `rustcodegraph`，并确认 MCP 配置指向 Rust 二进制命令；TypeScript 版本时期的 CodeGraph 配置不会自动复用。

## 更多

- [完整文档](docs/user/README.md)
- [更新日志](CHANGELOG.md)

## 许可证

MIT
