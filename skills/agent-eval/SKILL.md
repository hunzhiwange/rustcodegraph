---
name: agent-eval
description: 通过比较 agent 在有 RustCodeGraph 和没有 RustCodeGraph 时的行为，在真实代码库上基准测试 RustCodeGraph 的检索质量。当用户运行 /agent-eval，或要求针对某个语言仓库测试、基准测试、审计或验证一个 rustcodegraph 版本（本地开发构建或已发布的 npm 版本）时使用。
---

# RustCodeGraph 质量审计

衡量在选定的真实仓库和选定的 rustcodegraph 版本上，RustCodeGraph 相比普通 grep/read 能给 agent 带来多少帮助。驱动 `scripts/agent-eval/` 中的 harness。

## 前置条件

- `tmux` 3+、已登录的 `Codex` CLI、`node`、`git` 可用（macOS/Linux）。
- 从 RustCodeGraph 仓库根目录运行。

## 工作流

复制这份检查清单：

```text
- [ ] 1. 选择版本（local 或 npm）
- [ ] 2. 选择语言
- [ ] 3. 按规模选择仓库
- [ ] 4. 选择 harness（headless / tmux / both）
- [ ] 5. 在后台运行 audit.sh
- [ ] 6. 报告结果
```

**步骤 1 - 版本。** 使用 `AskUserQuestion` 询问要测试哪个 rustcodegraph 版本。提供 "Local dev build" 和 "Latest published"；自由文本 "Other" 允许用户输入特定版本（例如 `0.7.10`）。把答案映射成 VERSION token：

- "Local dev build" -> `local`
- "Latest published" -> `latest`
- 用户输入的版本 -> 原字符串（例如 `0.7.10`）

**步骤 2 - 语言。** 读取 `./skills/agent-eval/corpus.json`。使用 `AskUserQuestion` 询问要测试哪门语言，列出有条目的语言。

**步骤 3 - 仓库。** 从所选语言的条目中询问要测试哪个仓库。每个选项用规模和文件数标注，例如 `excalidraw - Medium (~600 files)`。每个条目都带有 `repo` URL 和一个代表性 `question`。

**步骤 4 - harness。** 使用 `AskUserQuestion` 询问要运行哪个 harness，并把答案映射成 MODE token：

- "Headless" -> `headless` - 使用 stream-json 的 `Codex -p`：精确 token/成本和干净的 tool 序列（2 次运行，速度快，无 TTY）。
- "Interactive (tmux)" -> `tmux` - 在 tmux 中驱动真实 Codex TUI：忠实反映 Explore-subagent 行为，指标来自 session log（2 次运行，较慢）。
- "Both" -> `all` - headless + interactive（4 次运行）。

**步骤 5 - 运行。** 在后台启动（设置版本、按需 clone、清空并重新索引、运行所选 arm，耗时数分钟）：

```bash
scripts/agent-eval/audit.sh <VERSION> <repo-name> <repo-url> "<question>" <MODE>
```

**步骤 6 - 报告。** 作业完成后，读取日志并按 arm 报告：

- Headless（`parse-run.mjs`）：总 tool call、文件 `Read`、Grep/Bash、rustcodegraph tool call、耗时、**总成本**。
- Interactive（`parse-session.mjs`）：`VERDICT: rustcodegraph_explore used Nx | Read N | Grep/Bash N` 和 `TOKENS:` 行。

优先报告成本以及 tool/Read 数量，它们是可靠信号；原始 token in/out 会被 subagent 委派和 prompt caching 干扰。说明 RustCodeGraph 是否降低了工作量，以及两个 arm 是否都得出了正确答案。

## 备注

- 每次运行都会重建索引（`audit.sh` 会清空 `.rustcodegraph`）。不同版本的抽取结果不同，因此索引必须由构建它的同一个 binary 提供服务。
- `audit.sh` 会在测试期间临时修改全局 `rustcodegraph` 安装，然后通过 `local-install.sh` 恢复你的开发链接。
- Corpus 仓库会 clone 到 `/tmp/rustcodegraph-corpus`（如果已经存在则复用）。
- 在 `corpus.json` 中添加或编辑仓库（字段：`name`、`repo`、`size`、`files`、`question`）。
