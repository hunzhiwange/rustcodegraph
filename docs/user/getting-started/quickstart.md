# 开始使用

只需几秒钟即可启动并运行 RustCodeGraph。

## 无需 Node.js — 一个命令即可为您的操作系统获取正确的版本

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh

# Homebrew (macOS / Linux)
brew install hunzhiwange/tap/rustcodegraph

# Windows (PowerShell)
irm https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.ps1 | iex
```

## 已经有节点了吗？ 使用 npm 代替（适用于任何版本）

```bash
npm i -g rustcodegraph
```

RustCodeGraph 提供原生 Rust 二进制文件——无需编译，无需 Node 运行时
 安装后。 然后运行 `rustcodegraph install` 自动配置您的代理：
 克劳德代码、光标、Codex CLI、opencode、Hermes Agent、Gemini CLI、
 反重力 IDE，Kiro。

## 初始化项目

```bash
cd your-project
rustcodegraph init -i
```

就是这样——当 `.rustcodegraph/` 目录存在时，您的代理将自动使用 RustCodeGraph 工具。

接下来：构建 [Your First Graph](./your-first-graph.md)，或查看完整的 [Installation](./installation.md) 选项。
