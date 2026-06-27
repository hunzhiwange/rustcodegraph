# 安装

安装 RustCodeGraph 并配置您的 AI 编码代理。

## 1.运行安装程序

```bash
rustcodegraph install
```

安装程序将：

- 询问要配置哪些代理 - 自动检测来自 **Claude Code**、**Cursor**、**Codex CLI**、**opencode**、**Hermes Agent**、**Gemini CLI**、**Antigravity IDE** 和 **Kiro** 的已安装代理。 
- 提示在您的 `PATH` 上安装 `rustcodegraph`（以便代理可以启动 MCP 服务器）。 
- 询问配置是否适用于您的所有项目或仅适用于这个项目。 
- 编写每个选定代理的 MCP 服务器配置以及说明文件（例如 `CLAUDE.md`、`.cursor/rules/rustcodegraph.mdc`、`~/.codex/AGENTS.md`）。 
- 当 Claude Code 是目标之一时设置自动允许权限。 
- 初始化当前项目（仅限本地安装）。

## 非交互式（脚本/CI）

```bash
rustcodegraph install --yes                              # auto-detect agents, install global
rustcodegraph install --target=cursor,claude --yes       # explicit target list
rustcodegraph install --target=auto --location=local     # detected agents, project-local
rustcodegraph install --print-config codex               # print snippet, no file writes
```

| 旗帜 | 价值观 | 默认 |
|---|---|---|
| `--target` | `auto`、`all`、`none` 或 csv (`claude,cursor,…`) | 迅速的 |
| `--location` | `global`、`local` | 迅速的 |
| `--yes` | （布尔值） | 提示每一步 |
| `--no-permissions` | （布尔值）跳过 Claude 自动允许列表 | 的权限 |
| `--print-config <id>` | 转储一个代理的片段并退出 | — |

## 2. 重新启动代理

重新启动代理（Claude Code / Cursor / Codex CLI / opencode / Hermes Agent / Gemini CLI / Antigravity IDE / Kiro）以加载 MCP 服务器。

## 3. 初始化项目

```bash
cd your-project
rustcodegraph init -i
```

这会构建每个项目的知识图索引并连接任何项目本地代理表面，因此单个全局 `rustcodegraph install` 在您打开的每个项目中都有效。

## 支持的平台

每个版本都为所有三个桌面操作系统提供独立的本机构建，
 在 x64 和 arm64 上。 在 macOS/Linux 上使用独立安装程序 Homebrew，
 或者 `npm i -g rustcodegraph` 如果你更喜欢 npm。

| 平台 | 架构 | 安装 |
|---|---|---|
| 视窗 | x64、arm64 | PowerShell 安装程序或 npm |
| macOS | x64、arm64 | shell 安装程序、Homebrew 或 npm |
| Linux | x64、arm64 | shell 安装程序、Homebrew 或 npm |

## 卸载

改变主意了吗？ 一个命令可以从它配置的每个代理中删除 RustCodeGraph：

```bash
rustcodegraph uninstall
```

这会反转安装程序 — 从每个配置的代理中剥离 RustCodeGraph 的 MCP 服务器配置、指令和权限。 您的项目索引（`.rustcodegraph/`）保持不变； 使用 `rustcodegraph uninit` 删除每个项目的那些。 使用 `--target` 从特定代理中删除，或使用 `--yes` 以非交互方式运行。
