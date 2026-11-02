# 大文件 AST 与完整 Flow 验证

日期：2026-11-02。基线：3c809234（已包含符号地图压缩）。固定 Excalidraw commit：8bcdca4526867433f99fdfcb55a063668db0c9dc。模型要求沿用维护者确认的 Codex gpt-6.1-sol/high，两分支至少两次有效运行。

## 修复与确定性证据

JS/TS 文件在扫描器已有 1 MiB 上限内统一走 AST，不再因为超过 128 KiB / 5000 行而退回按行成员扫描。原生字段分类在 AST visitor 中执行，移除会把多行箭头方法覆盖为 property 的文本补扫。兼容 AST 的祖先和相邻节点链接共享不可变 Arc，保留完整祖先链，避免克隆父节点文本。提取版本从 24 升为 25，普通包版本未改。

Flow 原来在找到前三个任意端点对后停止，导致图已连通仍只显示短段。现在使用最多 8 跳、600 次扩展、每层 64 条候选的有界搜索，优先覆盖更多命名符号，最多允许一个未命名桥接节点。合成边直接展示机制与 registeredAt，JSX 接线记录真实 tag 行。源码排在 caller 列表和补充位置之前，防止汇总挤掉函数体。

实际索引第一次：738 文件、11553 节点、65429 边，167570ms，最大 RSS 433291264 字节。第二次（包含 JSX wiring 元数据及确定性遍历）：仍为 738 文件、11553 节点，65445 边，183787ms，最大 RSS 331874304 字节。节点没有增长；两次边数对应不同的 JSX 合成实现，不声称两次边集合完全一致。

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
