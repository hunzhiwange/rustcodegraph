---
name: add-lang
description: 为 RustCodeGraph 端到端添加 tree-sitter 语言支持：接入原生 Rust grammar 和 extractor，编写测试，然后在 3 个热门真实仓库上基准测试抽取质量和检索价值。当用户运行 /add-lang <language>，或要求在 RustCodeGraph 中新增/支持一门语言（例如 Lua、Elixir、Zig、OCaml）时使用。
---

# 向 RustCodeGraph 添加一门语言

把一门新的原生 tree-sitter 语言接入 RustCodeGraph 抽取流水线，证明它能在热门仓库中抽取真实符号，并证明它比没有 RustCodeGraph 更能帮助 agent。自主执行：选择仓库、运行基准测试、更新文档，然后报告。不要 commit、push、publish 或 tag。

参数是 `Language` 中使用的小写语言 token，例如 `lua`、`elixir` 或 `zig`。如果没有给出参数，询问要添加哪门语言。全程使用稳定的单 token 形式（用 `csharp`，不要用 `c#`）。

## 前置条件

- 从 RustCodeGraph 仓库根目录运行。
- `git`、`gh`、Rust stable，以及已登录的 Codex CLI 可用。
- 基准测试使用本地开发构建。在基准循环前先构建并链接一次：`npm run build && ./scripts/local-install.sh`。

## 工作流

复制这份检查清单，并按顺序完成：

```text
- [ ] 1. 确认语言；如果已支持则及早退出
- [ ] 2. 添加或选择一个原生 tree-sitter grammar crate
- [ ] 3. 用 Rust helper 做 grammar 健康检查并查看 AST
- [ ] 4. 在 Rust 中接入该语言
- [ ] 5. 构建并循环 verify-extraction，直到 PASS
- [ ] 6. 添加抽取测试，并让测试通过
- [ ] 7. 自动按规模层级选择 3 个热门仓库；加入 corpus.json
- [ ] 8. 对 3 个仓库全部做基准测试：抽取 + with/without A/B
- [ ] 9. 更新 README + CHANGELOG
- [ ] 10. 报告；不要 commit
```

## 步骤 1 - 确认 + 短路

检查该语言是否已经接入：

- `src/types.rs` - `Language` enum 和 `LANGUAGES`
- `src/extraction/grammars.rs` - `NATIVE_GRAMMAR_REGISTRY`、`EXTENSION_MAP`、`get_language_display_name` 和 `language_key`
- `src/web_tree_sitter.rs` - `native_language`
- `src/extraction/languages/index.rs` - extractor 模块导出和 `extractor_for`

如果语言已经受支持，跳过实现，直接进入基准测试以验证检索价值。

## 步骤 2 - 添加或选择 Grammar

使用原生 Rust tree-sitter crate，不要使用 `.wasm` grammar。把依赖加入 `Cargo.toml`，并在运行 add-lang helper 前接入 `src/web_tree_sitter.rs::native_language`。如果没有维护中的 Rust grammar crate，就停止并报告阻塞原因，不要交付半接入的语言。

对于公开 token 与 crate symbol 不同的语言，在 RustCodeGraph 中保持公开 token 稳定，并在 `native_language` 中做映射。

## 步骤 3 - 健康检查并查看 AST

创建一个语法有效的样例，覆盖函数、类/结构体、import、enum、变量和调用。然后使用 Rust 侧 helper：

```bash
cargo run --bin rustcodegraph -- add-lang check-grammar <lang> path/to/sample.ext
cargo run --bin rustcodegraph -- add-lang dump-ast <lang> path/to/sample.ext --depth=6
```

`check-grammar` 会通过 RustCodeGraph 的 Rust parser facade 加载原生 grammar，并反复解析样例。`dump-ast` 会打印带字段名的有界树视图，以及 named-node 频率表。使用频率表决定哪些节点类型应映射为函数、类、import、调用、变量和 type alias。

## 步骤 4 - 接入语言

按现有 Rust 风格完成接入编辑：

1. `Cargo.toml` - 添加 `tree-sitter-<lang>` 依赖。
2. `src/types.rs` - 添加 `Language` enum 变体，并把它加入 `LANGUAGES`。
3. `src/extraction/grammars.rs` - 把 token 加入 `NATIVE_GRAMMAR_REGISTRY`，把扩展名加入 `EXTENSION_MAP`，添加显示名称，并增加一个 `language_key` match 分支。
4. `src/web_tree_sitter.rs` - 在 `native_language` 中把 `Language` 变体映射到原生 grammar crate；如有需要，更新 `Language::load` 的 token 检测。
5. `src/extraction/languages/<lang>.rs` - 添加一个 `LanguageExtractor` 实现，参考最接近的现有语言。
6. `src/extraction/languages/index.rs` 和 `src/lib.rs` - 暴露新的 extractor 模块，并把 token 加入 `extractor_for`。

有时，当 grammar 以通用 extractor 看不到的方式嵌套声明名时，`src/extraction/tree_sitter.rs` 需要一个很小的语言特定分支。保持该分支狭窄，并用测试覆盖。

## 步骤 5 - 构建 + 验证循环

构建本地 Rust binary，索引一个样例仓库，然后验证抽取：

```bash
npm run build
( cd <sample-repo> && rustcodegraph init -i )
rustcodegraph add-lang verify-extraction <sample-repo> <lang>
```

如果未检测到该语言，或索引结果只有结构性的 file/import/export 节点，验证会失败。失败时，重新运行 `dump-ast`，修正 extractor 映射，重新构建、重新索引、重新验证，直到通过。

## 步骤 6 - 测试

在 `tests/extraction_test.rs` 中添加覆盖：

- 扩展名的语言检测
- 从内联源码中抽取代表性的函数/类/import/调用
- 任何你必须特判的 grammar 特定变量、方法、receiver 或 import 行为

运行：

```bash
cargo test --test extraction_test -- --test-threads=1
```

## 步骤 7 - 自动选择 3 个仓库 + 语料

不要询问，直接选择。先寻找候选仓库，再人工筛出三个确实以该语言为主的仓库，每个规模层级一个：

```bash
gh search repos --language=<lang> --sort=stars --limit 40 \
  --json fullName,stargazerCount,description
```

使用一个小型仓库（<~150 个文件）、一个中型仓库（~150-1500 个文件）和一个大型仓库（>~1500 个文件）。为每个仓库编写一个跨文件架构问题；如果评测使用该语料库，把这组条目加入 `./skills/agent-eval/corpus.json`。

## 步骤 8 - 对 3 个仓库全部做基准测试

先让开发构建在 PATH 上可用，然后循环：

```bash
npm run build && ./scripts/local-install.sh
scripts/add-lang/bench.sh <lang> <name> <url> "<question>" headless
```

`bench.sh` 会 clone 或复用仓库，清空并索引 `.rustcodegraph`，运行 `rustcodegraph add-lang verify-extraction`，然后通过 `scripts/agent-eval/run-all.sh` 执行检索 A/B。报告两组实验中的 tool call、文件 Read、Grep/Bash、RustCodeGraph tool call、耗时和成本。

## 步骤 9 - 文档 + CHANGELOG

- `README.md`：把该语言加入功能 bullet 和受支持语言表。
- `CHANGELOG.md`：在 `## [Unreleased]` 下的 `### New Features` 中添加一条友好的说明，从用户视角解释该语言支持。

## 步骤 10 - 报告

总结供审阅：

- 改动文件
- 每个仓库的抽取结果：文件数、节点数、边数、验证结果
- 每个仓库的 A/B 结果：with vs without RustCodeGraph
- 缺口或后续事项，例如未映射的节点类型或缺失的 framework edge

保持改动未提交。发布通过 GitHub Actions Release workflow 完成。

## 备注

- A/B 会启动真实付费的 Codex run。除非维护者明确另有要求，否则保持 model 和 effort 与仓库的 agent-eval 脚本一致。
- 不要为新的运行时支持使用 `.wasm` grammar 文件。Rust runtime 使用原生 tree-sitter crate。
- 索引必须由构建它的同一个 binary 提供服务。步骤 8 会先构建并链接开发 binary，因此满足这一点。
