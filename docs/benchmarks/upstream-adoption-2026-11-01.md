# 全计划本地回归证据汇总

2026-11-05，任务 144 / 批次 375；检查起点 `7edb17ec`，产品版本 1.2.11。
只读上游来源固定为 `15483713763f344ff30fb7df92bcf091c7bd9758`。

## 范围和依赖

遵循用户 2026-11-03 修订：不运行 CLI 认证、真实仓库回放、重复 Agent A/B、
指定模型、性能或 VM/Docker 实验；取消项未执行，不记为通过，也不作为阻塞。
本次行为结果仅代表本地 macOS 默认 features 的测试。

下列收口提交均经 `git merge-base --is-ancestor <commit> HEAD` 确认为当前祖先；
实现、测试及支持边界见 [覆盖 playbook](../design/dynamic-dispatch-coverage-playbook.md)。
前置证据来自提交与文档核对，不以编号任务文件删除判断完成。

| 依赖批次 | 能力 | 收口提交 |
| --- | --- | --- |
| 79 | Nix | `dbf4fe04` |
| 97 | Terraform | `ab0bd00c` |
| 115 | Solidity | `6c253f49` |
| 133 | Erlang | `e9d249c9` |
| 151 | VB.NET | `b6c65310` |
| 169 | COBOL | `f22e62e6` |
| 187 | CFML | `2518b38a` |
| 205 | ArkTS | `86d28ba0` |
| 218 | CUDA | `0e6e99a7` |
| 231 | Metal | `331c5322` |
| 244 | Redux/RTK | `8be6815e` |
| 257 | Vue stores | `6eea3231` |
| 270 | Celery | `c3403ae8` |
| 283 | Sidekiq | `0941ee4a` |
| 296 | MediatR | `ae27fca9` |
| 309 | Spring events | `f8e102bb` |
| 322 | Laravel events | `22f79c11` |
| 335 | HTTP tier | `a5b2f001` |
| 348 | Queue tier | `d8e0f984` |
| 361 | Socket tier | `7f33feae` |
| 374 | Tauri | `7edb17ec` |

基础优化批次 1–22 的提交与测试均存在：
`eabceb7b`、`e9fa5bea`、`191751fa`、`1cab37e7`、`cfb56aec`、`6f69020f`、
`272f2676`、`ae934a41`、`311802f5`、`a525556b`、`79b86a21`、`3246e6f2`、
`10f6dc76`、`969c3156`、`4a27a389`、`b8179aeb`、`6390e5a7`、`58c1d44d`、
`db2d32e1`、`e4d2588b`、`f9d1c055`、`3ebfff30`、`d42c7d5d`。
批次 32 的历史检索证据为 `763dd8e0`、`0bfa3586`，见
[检索控制报告](upstream-explore-2026-11-01.md)，本次未重复外部实验。

## 安装器与矩阵核对

| 平台依赖 | 已有证据 | 本次范围 |
| --- | --- | --- |
| macOS / 59 | `64e96da2`，2026-11-03 临时 HOME 路径/往返 163 passed | 本地主机条件测试可运行 |
| Linux / 60 | `f95b9be5`，2026-11-03 Docker 历史记录 163 passed | 本次未运行 Docker，不声明当前 Linux 通过 |
| Windows / 61 | 真实 VM 验收由用户取消，无通过记录 | 本次未运行，不补造平台证据 |

历史详情见 [安装器报告](upstream-installer-platforms.md)。macOS/Linux 专属路径模块有
`target_os` 门控；没有运行的条件编译测试不能计为本次通过或从 ignored 计数中推断覆盖。

21 类语言/框架原计划的 189 项外部验收均受取消修订约束。
Nix 保留 [九项固定输入](upstream-matrices/nix.json)（小、中、大各三个问题）及
[历史静态证据](upstream-matrices/nix-evidence.json)，实现版本为
`51057670b77b35cb25a9cef587deff82e3828315`，本次没有回放。
其余 20 份 JSON 为取消清单：`cases=[]`、`external_experiments_executed=false`、
`runner_compatible=false`，不是九案例矩阵通过报告。
逐项核对这些清单的 prerequisite commit 祖先关系及 implementation/local_checks 文件存在性。

新增 `upstream_375_test` 的两项非 ignored 检查保护取消清单的诚实状态、来源及本地文件引用，
并检查 Nix 历史九项输入的规模、问题与固定 commit；它们是证据完整性检查，不替代框架行为测试。
静态支持声明继续受 playbook 的各自限制约束，不承诺动态值、歧义解析、无注册证据派发或运行时投递。

## 回归诊断

首轮 `cargo test -p rustcodegraph` 退出 101，遇错停止前共
1691 passed、1 failed、81 ignored（96 个完成目标），日志 `/tmp/rcg-375-full.log`。
`upstream_011_test` 仍断言 Java 字段均没有 signature，
与 `f7167b90` 保留原生字段类型的修复冲突。

按后续用户授权，本次仅更新这一过时测试：选择原生 class 声明而非 facade 重复身份，
断言 Base.sink 为 Writer、Child.Inner.sink 为 Reader、缺失字段为 None，
清缓存后重复检查，并保留 Child 自身声明不得混入基类或嵌套字段的负例。
未修改生产实现或扩大语言/框架支持范围。
修正后 `cargo test -p rustcodegraph --test upstream_011_test` 退出 0：2 passed、0 ignored。

## 最终验证

所有恢复阶段动态领取槽位 1；绝对
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1`，
`CARGO_BUILD_BUILD_DIR` 为该目录下的 `build`。每阶段命令及子进程退出后释放自身锁，保留缓存。

| 命令 | 退出码 | 结果 | 本地原始日志 |
| --- | --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化完成 | `/tmp/rcg-375-fmt-audit.log` |
| `cargo test -p rustcodegraph --test upstream_011_test` | 0 | 2 passed，0 ignored | `/tmp/rcg-375-focus-java.log` |
| `cargo test -p rustcodegraph --no-fail-fast` | 101 | 2143 passed，1 failed，81 ignored | `/tmp/rcg-375-full-resume.log` |
| `cargo test -p rustcodegraph --test upstream_375_test` | 0 | 2 passed，0 ignored | `/tmp/rcg-375-focus-audit.log` |
| `cargo test -p rustcodegraph` | 0 | 2144 passed，0 failed，81 ignored，0 filtered | `/tmp/rcg-375-full-final.log` |

遇错继续的一轮唯一失败是新证据检查最初只接受字符串路径，未兼容既有 CUDA/Metal/Redux
清单的路径数组；修正为同时接受非空字符串或数组后，聚焦和最终原命令全量均通过。
最终日志 SHA-256：`06496a53423dbc8061ae692d88062a9ed5ab0436adb85e7e423efb7c615166ab`。

最终统计来自 204 个测试结果段（包括零测试目标及 doc-tests），不是把编译成功算作行为测试。
`upstream_*` 目标合计 388 passed、0 failed、0 ignored，新增 375 的两项均实际执行。
安装器套件 163 passed、0 ignored，包含 macOS 路径往返；其他平台的门控测试未运行。
81 项 ignored 来自 daemon attach 辅助入口 1 项、resolution 78 项、security 2 项，均非本次新增，
未记为通过。测试集合的绿色结果不覆盖这些跳过项或已取消的外部验收。

本次全计划本地回归已完成；证据支持已启用静态能力的当前本地检查，
不新增跨平台、真实仓库矩阵、完整运行时语义或性能保证。
