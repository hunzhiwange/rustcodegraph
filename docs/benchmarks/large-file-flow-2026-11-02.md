# 大文件 AST 与完整 Flow 验证

日期：2026-11-02。基线：3c809234（已包含符号地图压缩）。固定 Excalidraw commit：8bcdca4526867433f99fdfcb55a063668db0c9dc。模型要求沿用维护者确认的 Codex gpt-6.1-sol/high，两分支至少两次有效运行。

## 修复与确定性证据

JS/TS 文件在扫描器已有 1 MiB 上限内统一走 AST，不再因为超过 128 KiB / 5000 行而退回按行成员扫描。原生字段分类在 AST visitor 中执行，移除会把多行箭头方法覆盖为 property 的文本补扫。兼容 AST 的祖先和相邻节点链接共享不可变 Arc，保留完整祖先链，避免克隆父节点文本。提取版本从 24 升为 25，普通包版本未改。

Flow 原来在找到前三个任意端点对后停止，导致图已连通仍只显示短段。现在使用最多 8 跳、600 次扩展、每层 64 条候选的有界搜索，优先覆盖更多命名符号，最多允许一个未命名桥接节点。合成边直接展示机制与 registeredAt，JSX 接线记录真实 tag 行。源码排在 caller 列表和补充位置之前，防止汇总挤掉函数体。

实际索引第一次：738 文件、11553 节点、65429 边，167570ms，最大 RSS 433291264 字节。第二次（包含 JSX wiring 元数据及确定性遍历）：仍为 738 文件、11553 节点，65445 边，183787ms，最大 RSS 331874304 字节。节点没有增长；两次边数对应不同的 JSX 合成实现，不声称两次边集合完全一致。

最终新索引的合成边为 callback 1、react-render 46、jsx-render 310、interface-impl 14。对目标链逐边 SQL 抽查确认：Scene.mutateElement→triggerUpdate 为静态调用；callback 的 onUpdate/callbacks 配对及 App.tsx:3858 注册行匹配源码；triggerRender→App.render 标记 react-render，函数体确有 setState；App.render→StaticCanvas 的 JSX 接线位置为 App.tsx:2618；StaticCanvas→renderStaticScene 为静态调用。未增加所有局部变量节点。App.tsx 节点数由 5188 降到 413。

旧完整索引为 17697 节点。新 App.tsx 中 if/return 伪成员消失，triggerRender 为 method、5080–5089 行；数据字段仍为 property。单独原生解析 App.tsx 的峰值 RSS 从 3846537216 字节降到 148668416 字节；新原生探针耗时约 1.84s，仍提取 413 个节点、无错误。前值是旧实现的 time -l cargo test（最大子进程 RSS），后值直接计时同一诊断测试可执行文件，不用编译时间比较运行性能。临时诊断测试已移除，避免提交依赖本机副本的测试。

真实冻结查询保持不变：

```text
mutateElement triggerUpdate triggerRender render StaticCanvas renderStaticScene
```

新 Flow 第一条包含 Scene.mutateElement → Scene.triggerUpdate → [callback] App.triggerRender → [react-render] App.render → [jsx-render] StaticCanvas → renderStaticScene → _renderStaticScene。三处 wiring 分别为 App.tsx:3858、2325、2618。查询输出中 triggerRender 函数体及 App.tsx 源码均存在。原始探针 /tmp/rcg-largeflow/explore-final.log，SQLite /tmp/rcg-largeflow/excalidraw/.rustcodegraph/rustcodegraph.db。

## 测试与基线故障审计

新增真实临时文件 + parser + SQLite 测试涵盖：大 TSX 无伪成员、多行箭头方法范围与数据字段、大 JS 字段分类、整条 callback/render/JSX Flow 先于短段、caller 汇总不能挤掉源码、禁止两条未命名桥接；另有 clone 后完整祖先链与文本的验证。大文件抽取、完整 Flow 排序、源码保护分别先复现行为失败，日志 large-ts-red.log、flow-red.log、source-red.log 位于 /tmp/rcg-largeflow 或此前 /tmp/rcg-control032。

cargo fmt --all 后，cargo test --no-fail-fast：1773 passed（其中 1 个临时真实仓库诊断测试随后移除）、2 failed、81 ignored；127 个 target 结果。仅 frameworks_integration_test 两个 Java 注入调用链用例失败。已在固定基线的独立 git archive 上重跑同一组，3 passed、同样 2 failed，因此这两项不是本次新回归。初次 cargo test 也停在同一组；未将全套结果写成全绿。

动态 Cargo 槽位：真实新索引与最终广泛回归使用槽位 1；初次广泛回归及基线 Java 审计使用槽位 2。CARGO_TARGET_DIR 分别为 /Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1 与 rust-slot-2 的绝对路径；CARGO_BUILD_BUILD_DIR 均为对应目录的 build。所有阶段锁均释放，缓存保留。完整命令及原始输出见 /tmp/rcg-largeflow/{cargo-test.log,cargo-test-all.log,baseline-java.log,index.log,reindex.log,native-memory.log}。

基线审计命令：在 /tmp/rcg-largeflow/baseline-src 中运行 cargo test --test frameworks_integration_test java_end_to_end_field_injected_bean_trace_issue_389 -- --nocapture，退出 101。探针命令：engine-new agent-eval probe-explore /tmp/rcg-largeflow/excalidraw 加上述查询，退出 0。真实索引命令：engine-new init -i /tmp/rcg-largeflow/excalidraw，两次退出 0。

## A/B 设计

使用相同源码、相同问题、相同 gpt-6.1-sol/high 和 explore-first 提示。区别是 baseline 从基线引擎与旧完整索引启动，new 从新引擎与新完整索引启动。脚本支持分别传入 --baseline-repo 和 --new-repo，避免拿同一旧索引比较提取改动。每轮独立副本、预热 daemon、MCP required=true；只计实际成功调用 MCP 并完整结束的运行。原始记录写入 /tmp/rcg-largeflow/ab-wired-*，后续容量失败须保留并按同一模型补足有效样本。

## A/B 实测结果

新引擎对应提交 02ab02bd；基线对应 3c809234。两边都已有同一符号地图压缩。基线使用 /tmp/rcg-control032/excalidraw 的 17697 节点旧完整索引，新版使用 /tmp/rcg-largeflow/excalidraw 的 11553 节点新索引，源代码是同一个固定 commit。本次无容量失败，无需补跑。四轮均退出 0，全部实际调用 MCP，tool error=0。

| 引擎 | 耗时秒 | MCP 调用 | 总工具调用 | 非 MCP 源码 Read / Grep | input / output tokens |
|---|---:|---:|---:|---|---|
| baseline 1 | 179.3 | 31 | 32 | 0 / 0 | 598240 / 3870 |
| baseline 2 | 140.8 | 35 | 37 | 0 / 0 | 407122 / 4122 |
| new 1 | 114.9 | 29 | 30 | 0 / 0 | 338632 / 3584 |
| new 2 | 123.2 | 24 | 25 | 0 / 0 | 366851 / 3473 |

每分支 n=2。耗时范围为基线 141–179s、新版 115–123s；均值 160.0→119.1s，本样本减少 25.6%。总工具调用均值 34.5→27.5（减少 20.3%）；MCP 均值 33→26.5。input+output token 均值 506677→356270（减少 29.7%），cached_input 是 input 子集，未重复相加。token 值保存原始 Codex turn.completed 的整用户轮合计，不据此宣称已获得逐次内部 assistant response 的 usage 或真实美元成本。

逐条检查四轮 shell 命令：每轮一次读取 rustcodegraph 技能说明，baseline 2 另有一次 status 检查；没有仓库源码 cat/sed/rg 等读取。总工具次数按 item.completed 中 MCP+shell 计数，不把 started/completed 重复累加。摘要、完整 shell 命令与 usage 原始字段见相邻 large-file-flow-2026-11-02-results.json。

两个新版本的首个 explore 响应都包含完整六个命名符号的 Flow，且包含 this.setState({}) 的 triggerRender 方法体；基线首个 explore 均缺少完整路径与该方法体。输出直接使用冻结的原始 symbol bag，无需换查询写法。

结果边界：这证明该固定仓库、该问题和这四轮运行中，源码 Read/Grep 未增加，耗时、工具次数与 token 均改善；不是小/中/大仓库全矩阵或所有语言的性能保证。新版本仍使用 24–29 次 MCP，未达到中型仓库正常流程 3–5 次工具调用的目标；余下调用大部分用于 nonce、memoization、画布缓存和更深的 draw 细节，未尝试把每个局部变量都建入图。单轮结果不可替代重复测量。

实际启动命令（依赖已配置的本机代理）：

```sh
HTTPS_PROXY=http://127.0.0.1:7897 HTTP_PROXY=http://127.0.0.1:7897 ALL_PROXY=http://127.0.0.1:7897 python3 scripts/agent-eval/run-codex-control.py /tmp/rcg-largeflow --baseline-repo /tmp/rcg-control032/excalidraw --new-repo /tmp/rcg-largeflow/excalidraw
```

该命令及四个子运行均退出 0。每轮的 run.jsonl/run.err/daemon.log/meta.json 位于 /tmp/rcg-largeflow/ab-wired-{base,new}-{1,2}/；预热 daemon 在每轮 finally 中回收。A/B 开始前已完成编译、索引与全套回归，未把同机编译负载混进计时。
