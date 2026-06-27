# RustCodeGraph A/B 基准测试 — 有与无，每种语言 × S/M/L

**日期：** 2026-05-24 · **分支：** `main` · **rustcodegraph 0.9.4**

无头特工（Claude Opus，`--permission-mode bypassPermissions`）回答了一个问题
**规范流程问题**每个存储库 - 两次：**使用** rustcodegraph MCP 服务器，以及
**没有**任何 MCP（仅限内置 Read/Grep/Glob/Bash）。相同型号，相同提示； Rust 代码图
是唯一的变量。首先对每个单元进行**重新索引新鲜**（针对 `dist/` 构建）
当前的 `main` HEAD），因此“带”臂反映了已发货的 0.9.4 解析器。

## 标题

**在 37 个单元中，rustcodegraph 将文件读取总数从 159 → 38 减少了 — 减少了 76%。** 它从来没有
*增加*任何单元格中的读数（0 回归）。机制：几个亚毫秒级的 rustcodegraph
调用取代了 read-and grep 探索。

**成本保持大致持平 - 这里的带臂成本略高**（37 个项目的总和）
细胞：使用 `$15.4` 与不使用 `$13.8`）。在这些简短的单流问题上，无臂
在 <10 次调用中解析并且永远不会膨胀，因此它不会达到 rustcodegraph 成本的范围
节省复合，而 with-arm 支付固定的 MCP 开销（上下文中的工具定义 +
工具加载），短期任务不会摊销。胜利在于**更少的工具调用（189 vs 321，-41%）
+ 较低的挂钟**（意味着**38秒与48秒**），这是设计目标。较难的多圈
随着没有手臂的人积累的背景信息不断膨胀，调查成本转变为净节省——
参见 `docs/benchmarks/call-sequence-analysis.md`。

差距随着回购规模和流程复杂性而扩大：在中/大型回购上，没有 rustcodegraph
手臂经常 **thrashes** — 许多 grep/glob、shell `find`/`grep` (Bash)，并且偶尔会生成
**子代理** — 而 with-rustcodegraph 手臂则在 2-8 个呼叫中应答。在小型回购协议上（少数
文件）两条手臂的领带或 rustcodegraph 稍微慢一些（MCP/索引开销没有得到回报
当整个流程适合一两个文件时） - 但读取仍然下降。

## 如何读表

- **R / G / Gl / B / Ag** = Read / Grep / Glob / Bash / 子代理（任务）工具调用。
- **cg-calls** = rustcodegraph MCP 在“with”臂中调用（读取/greps 的交易）。
- **dur** = 挂钟秒数。 **文件** = 索引文件计数（大小代理）。
- **保存的读数** = 不带读数 - 带读数。
- 每臂运行一次（**快照** — 运行间差异是真实的；将 ±1–2 次读数和 ±10 秒视为
噪声，查看单元格之间的模式）。其中几个流程的 2 次/臂标题数字
住在 `docs/design/dynamic-dispatch-coverage-playbook.md` §7。

## 结果

| 语言 | 尺寸 | 回购协议 | 文件 | **带** R/G | cg 呼叫 | 杜尔 | **无** R/G | 杜尔 | 读取已保存 |
|---|---|---|--：|---|--：|--：|---|--：|--：|
| C | L | `c-redis` | 第884章 | 0R/2G | 4 | 42秒 | 5R/6G | 51秒 | 5 |
| C# | S | `aspnet-realworld` | 78 | 0R/0G | 2 | 27秒 | 5R/3G/2Gl | 54秒 | 5 |
| C# | 中号 | `aspnet-eshop` | 262 | 0R/1G | 5 | 39秒 | 9R/2G/5Gl | 58秒 | 9 |
| C# | L | `aspnet-jellyfin` | 2081 | 3R/0G | 4 | 51秒 | 17R/1G/2Gl/17B/1Ag | 212秒 | 14 |
| C++ | 中号 | `cpp-leveldb` | 134 | 0R/0G | 3 | 26秒 | 4R/2G | 37秒 | 4 |
| Dart | S | `flutter_module_books` | 6 | 1R/0G | 2 | 24秒 | 2R/0G/1Gl | 29秒 | 1 |
| Dart | 中号 | `compass_app` | 212 | 2R/0G/1Gl | 2 | 42秒 | 3R/0G/2Gl | 30秒 | 1 |
| Go | S | `gin-realworld` | 21 | 0R/0G | 5 | 35秒 | 4R/3G/1Gl | 57秒 | 4 |
| Go | 中号 | `gin-vueadmin` | 625 | 1R/1G | 4 | 47秒 | 3R/3G/1Gl | 44秒 | 2 |
| Go | L | `gin-gitness` | 4438 | 4R/3G | 4 | 64秒 | 8R/7G/2Gl | 57秒 | 4 |
| Java | S | `spring-realworld` | 117 | 2R/0G | 3 | 35秒 | 8R/1G/5B | 57秒 | 6 |
| Java | 中号 | `spring-mall` | 第536章 | 1R/0G | 5 | 39秒 | 2R/4G/2Gl | 49秒 | 1 |
| Java | L | `spring-halo` | 2444 | 1R/2G | 8 | 60年代 | 4R/1G/6B | 52秒 | 3 |
| Kotlin | S | `kotlin-petclinic` | 43 | 0R/0G | 2 | 37秒 | 3R/0G/1Gl | 23秒 | 3 |
| Kotlin | 中号 | `Jetcaster` | 166 | 1R/0G | 3 | 36秒 | 1R/0G/2Gl | 46秒 | 0 |
| Lua | S | `lualine.nvim` | 123 | 1R/1G | 4 | 48秒 | 4R/0G/2Gl | 49秒 | 3 |
| Lua | 中号 | `telescope.nvim` | 84 | 0R/0G | 1 | 15秒 | 1R/0G/1Gl | 20多岁 | 1 |
| Luau | S | `Knit` | 11 | 0R/0G | 2 | 30秒 | 5R/0G/2Gl | 37秒 | 5 |
| PHP | S | `laravel-realworld` | 114 | 1R/0G | 6 | 40多岁 | 5R/1G/3Gl | 39秒 | 4 |
| PHP | 中号 | `laravel-firefly` | 2047 | 2R/1G | 4 | 47秒 | 4R/5G/3Gl | 75秒 | 2 |
| PHP | L | `laravel-bookstack` | 2160 | 1R/2G | 2 | 41秒 | 2R/4G/1Gl | 50多岁 | 1 |
| Python | S | `django-realworld` | 44 | 2R/1G | 2 | 47秒 | 9R/0G/1B | 38秒 | 7 |
| Python | 中号 | `django-wagtail` | 第1672章 | 2R/0G | 4 | 45秒 | 8R/3G/3Gl/1B | 66秒 | 6 |
| Python | L | `django-saleor` | 4429 | 2R/2G | 4 | 52秒 | 4R/6G/1Gl | 64秒 | 2 |
| Ruby | S | `rails-realworld` | 59 | 0R/0G | 2 | 30秒 | 3R/0G/2B | 33秒 | 3 |
| Ruby | 中号 | `rails-spree` | 2905 | 2R/3G/1Gl | 5 | 43秒 | 3R/3G/2Gl/1B | 55秒 | 1 |
| Ruby | L | `rails-forem` | 4658 | 3R/1G | 3 | 43秒 | 4R/2G/3Gl | 48秒 | 1 |
| Rust | S | `rust-axum-realworld` | 13 | 0R/0G | 2 | 21秒 | 3R/0G/1Gl | 38秒 | 3 |
| Rust | 中号 | `rust-actix-examples` | 176 | 0R/1G | 3 | 42秒 | 3R/0G/3B | 36秒 | 3 |
| Rust | L | `rust-cratesio` | 1053 | 1R/0G | 3 | 22秒 | 1R/2G | 18秒 | 0 |
| Scala | S | `computer-database` | 10 | 1R/0G | 2 | 27秒 | 3R/0G/1Gl | 25秒 | 2 |
| Swift | S | `vapor-template` | 14 | 0R/0G | 2 | 21秒 | 2R/0G/2Gl | 22秒 | 2 |
| Swift | 中号 | `vapor-steampress` | 100 | 0R/0G | 5 | 49秒 | 3R/1G/2Gl | 39秒 | 3 |
| Swift | L | `vapor-spi` | 第542章 | 1R/1G | 4 | 27秒 | 2R/5G | 34秒 | 1 |
| TypeScript/JS | S | `express-realworld` | 39 | 1R/0G | 1 | 25秒 | 2R/2G | 19秒 | 1 |
| TypeScript/JS | 中号 | `excalidraw` | 第643章 | 1R/0G | 3 | 55秒 | 7R/5G/3Gl/1B | 87秒 | 6 |
| TypeScript/JS | L | `nest-immich` | 2759 | 1R/0G | 7 | 50多岁 | 3R/0G/1Gl | 44秒 | 2 |

**总计（37 个单元格）：** 使用 rustcodegraph **38 个读取/22 个 grep**，没有 **159 个读取/72 个 grep** —
** 读取次数减少 76%，greps 减少约 69%。** Codegraph 从未增加任何单元格中的读取次数，并且
without-arm 还运行 **52 个球体 + 37 个 shell `find`/`grep` (Bash) + 1 个子代理**
with-arm (**0 Bash, 0 sub-agent**) 永远不需要。 （74 次代理运行，总计 29.18 美元。）

## 观察结果

- **最大的胜利是具有真实路由→处理程序→服务流的中型/大型后端：** aspnet-jellyfin
（3R / 51s vs **17R + 17 Bash + 生成的子代理 / 212s** - 最引人注目的单个单元），
aspnet-eshop（0R 与 9R）、django-realworld（2R 与 9R）、spring-realworld（2R 与 8R + 5 Bash）、
django-wagtail（2R 与 8R）、excalidraw（1R/55s 与 7R/87s）、Luau Knit（0R 与 5R）、aspnet-realworld
（0R 与 5R），c-redis（0R 与 5R）。
- **如果没有 rustcodegraph，大型存储库会使代理崩溃：** 它会退回到 shell `find`/`grep`
（跨矩阵的 37 个 Bash 调用）在 jellyfin 上甚至产生了一个子代理——这正是行为
rustcodegraph 的目的是防止。 with-arm 应答 2-8 个 rustcodegraph 调用并使用 **0 Bash
和 0 个分代理** 任何地方。
- **Tie zone = 小型存储库**（Kotlin Jetcaster 1R/1R、Rust cratesio 1R/1R、express 1R/2R、Swift 模板
0R/2R)：整个流程适合 1-2 个文件，因此读取已经很便宜； rustcodegraph 与读取相关并且是
有时慢几秒（MCP + 索引开销 — Kotlin petclinic 37s vs 23s，cratesio 22s vs
18 秒）。这与 rustcodegraph 的值随存储库大小缩放的设计说明相匹配。
- **持续时间跟踪大型存储库上的读取**（jellyfin 51s 与 212s、excaldraw 55s 与 87s、aspnet-eshop
39s 与 58s、django-wagtail 45s 与 66s），并且对小尺寸来说是噪音；平均挂钟为 38 秒 vs 48 秒
没有。
- 一些“with”单元仍然读取 2-4 个文件（jellyfin、gitness、forem、saleor、django）——剩余的为
记录的前沿（匿名处理程序、深层服务链、动态查找器）； rustcodegraph 得到
代理到正确的文件，然后它读取一个文件以确认细节。

## 覆盖范围说明

所有 14 个自述文件框架和每种与流程相关的语言都经过验证（请参阅手册）。这
这里的大小是按索引文件数计算的；一些语言在语料库中缺乏干净的第三种尺寸
（Dart/Kotlin = S/M，Scala/Luau = 仅 S，C = L，C++ = M）——这些单元格被省略
比伪造的。

## 复制

规范线束：`scripts/agent-eval/run-all.sh <repo> "<question>" headless`（带有 = rustcodegraph-only
MCP，没有=空MCP），从stream-json日志中解析。使用的一次性矩阵驱动程序+解析器
对于此表，`/tmp/ab-matrix/`：`run.sh`（`lang|size|repo|question` 矩阵 - 每个单元格
`rm -rf .rustcodegraph && rustcodegraph init -i` 然后是双臂），`parse-matrix.mjs`（单元格→此表），以及
`compare.mjs`（旧与新差异+聚合）。首先从目标提交构建 `dist/`，以便 MCP
服务器加载被测试的代码（PATH 上的 `rustcodegraph` 是 `npm link` 到开发 `dist/`）。
