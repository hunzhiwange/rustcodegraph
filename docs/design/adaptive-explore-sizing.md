# 设计 + 状态：自适应 `rustcodegraph_explore` 尺寸（兄弟骨架化）

**状态：**已实施和验证，**默认开启**，在分支上
`feat/adaptive-explore-sizing`（初始提交 `d6d059f`；**2026-05-29 完善**
在真实代理 A/B 暴露回读回归之后 — 请参阅
下文“细化”）。逃生舱口：`RUSTCODEGRAPH_ADAPTIVE_EXPLORE=0`。
**动机：**使 `rustcodegraph_explore` 调整其输出到*答案*的大小
比总是填满预算上限——所以“兄弟姐妹多”的流动（许多
一个接口的可互换实现）不再比
简单的 grep/read，不会导致真正需要广泛的“扩散”流
来源。

> **细化 (2026-05-29) — 回读回归。** 第一次剪切门控
> 仅在*off-spine + 多态兄弟*上。真实代理人 A/B（不是
> 确定性探针）表明，这将代理的两个文件骨架化了
> **回读**，打败了要点：OkHttp 的 `RealCall` （它实现了
> 9-impl `Lockable` *mixin*，所以它触发了同级信号，即使它是
> 协调器）和 Django 的 `compiler.py` （它 *定义* `SQLCompiler` 和
> 与其子类并置）。两个条件解决了这个问题——文件仅骨架化
> 如果它**没有幸免**，其中**幸免=代理在其中命名为可调用**
> （`getResponseWithInterceptorChain`、`SQLCompiler.execute_sql` → 保持满）
> **除非文件定义了 ≥3-impl 超类型**（基类+子类“家族”
> 文件很大并且无论如何都可以读取，因此将其骨架化*可以释放探索预算*
> 否则代理会读取同级文件）。结果：OkHttp **3%
> 成本更高 → 便宜约 10%**（RealCall 已满，0 回读）； Django **贵10%
> → 便宜约 10%**（compiler.py 框架释放了 28 KB 预算中的约 6.5 KB；一半
> 运行结果为 0 个读数）。超类型信号最初被用作
> *备用*——这是倒退的，Django 因挨饿而成本降低了 9%
> 其预算；它现在是命名可调用备用的“覆盖”。这
> 保留下面的单一条件历史记录以供上下文使用。

> **进一步细化 (2026-05-29) — 每个符号聚焦视图 + 命名簇
> ** 整个文件的骨架/备用在真正的 Django 上仍然太粗糙
> A/B：代理读回 `compiler.py`（折叠 → 其 `execute_sql`/`as_sql`
> 身体被省略）和 `query.py` （一个非兄弟的上帝文件，其 `_fetch_all` 簇
> 被修剪了）。四项更改使两个回购协议的价格从 ~9–10% 降至 **~14–17%**
> **中位数 0 读取**：
> 1. **唯一性感知备件** — 只有（接近）唯一的命名可调用备件
>    文件。 `as_sql` 在每个编译器/表达式子类中都有 **110 个定义**；
>    命名它不能让每个后端变体都充满（它淹没了 Django
>    预算）。 `getResponseWithInterceptorChain` (1 def) 仍然保留 RealCall。
> 2. **每个符号聚焦视图** - 折叠的族文件显示**全身**
>    脊柱上/唯一命名/规范基超类型方法，并且仅
>    其余的**签名**。所以 `SQLCompiler.execute_sql`/`as_sql` 生存
>    而 80 个其他符号 + 冗余子类崩溃 → 没有回读。
> 3. **所有层上的测试文件排除** — 测试文件 (`custom_lookups/tests.py`)
>    消耗了 Django 28 KB 预算中的 2.3 KB；测试很少回答
>    架构问题。 （以前只有 <500 个文件的层将它们排除在外。）
> 4. **非同级文件中的命名集群生存** - 注入代理命名方法
>    即使收集错过了它们，也会将 defs 放入文件的簇中，将它们排名为
>    重要性 9，以及 `min(per-file, remaining-total)` 处的帽簇选择
>    因此，高度重要的命名簇得以生存，而不是按源顺序排列
>    修剪（Django 的 `_fetch_all`，L2237，发出的四个大文件中的最后一个）。
> 控制权：OkHttp 便宜 14% / 0 RealCall 回读；超凡抽奖 31%
> 更便宜 / 0 读取（上帝文件集群不受影响 - 它的大文件被发出
> 首先，所以预算上限永远不会约束它）。 OkHttp 的拦截器保持纯粹
> 签名骨架（其中没有命名可调用，不定义超类型）。

---

## 长话短说

`rustcodegraph_explore` 返回 **每个** 相关文件的完整源代码
字符预算。对于一个答案跨越许多“相同形状”类别的问题 - 例如
《OkHttp如何通过其拦截器链处理请求？》，这涉及到
大约 14 个 `class … : Interceptor` 实现 — 这意味着大约 28 KB
**多余的全身**。因为这些物体在上下文窗口中运行
在会话的其余部分，WITH-RustCodeGraph 臂的成本*高于*WITHOUT 臂
（它用大约 10 个廉价的 grep 回答了著名的拦截器问题）。好的http
是基准的成本异常值（−3%——即比本机搜索*成本更高*）。

修复：当文件**同时 (a) 离开合成流脊线和 (b) a 时
多态兄弟**，将其渲染为**骨架**（类+成员*签名*，
主体被省略）而不是完整的源代码 - 保留脊柱上的范例和
机制健全。

- **OkHttp：**拦截器链流程骨架化了5个冗余
`: Interceptor` 意味着同时保持 `RealInterceptorChain` （调度
机制）和 `RealCall`（代理命名的协调器）完整 → **~10%
比本机便宜，0 RealCall 回读**（请参阅细化以了解更正的
数字；最初的 `28.5k → 16.6k` /“读 1 vs 3”数字来自
确定性探测查询，而不是代理的真实查询）。
- **Django：** QuerySet→SQL 流程骨架化了 `compiler.py`（a
基+子类系列文件），释放预算 → **~便宜 10%**。 （较早的
声称 Django 是“字节相同/0 骨架”的说法是
*探测*查询；代理的真实查询确实显示了 SQLCompiler 系列。）
- **Excalidraw / Tokio / VS Code / Gin：**探索输出是**字节相同**
标志打开/关闭（0 个骨架）——它们的流程没有偏离主干
≥3 个实施者兄弟组。修正后的门仅*添加*一个备用门
条件，因此它骨架化了原始门的**严格子集**→这些
存储库可证明保持在 0 个骨架（通过探针验证）。

---

## 一张图片中的问题

`format_explore_file_result` 收集相关文件，按相关性排序，并填充到
`maxOutputChars`（“整个小文件规则”转储任何≤220行的相关文件
全文）。预算是**目标**，而不是上限：

```
OkHttp explore (shipped):  RealCall (full) + RealInterceptorChain (full)
                         + CallServerInterceptor (full, 8.7k)
                         + Bridge/Connect/Cache/… (full, ~4-5k each)   ← all ~same shape
                         = ~28k, most of it redundant interceptor bodies
```

代理只需要**机制**（`RealInterceptorChain.proceed`迭代
链）+每个拦截器实现的**合约**+也许是一个具体的
例子。其他五个完整的身体正在填充——但只是*因为它们是
可互换*。关于一个分散的问题（Excalidraw 的渲染管道：
`mutateElement → … → renderStaticScene`)，off-spine 文件是**不同的
步骤**，并且他们的身体确实起作用——忽略它们只会使代理
根据签名重建它们（更多推理，净成本更高；请参阅“死胡同”）。

所以整个游戏是：**告诉“可互换的兄弟姐妹”与“不同的兄弟姐妹”
一步，“便宜。**

## 大门（精制）

文件被骨架化当且仅当 **all** 保留（和 `RUSTCODEGRAPH_ADAPTIVE_EXPLORE != 0`）：

1. **存在命名流查询。** `format_flow_section` / `find_named_flow_path`
渲染命名查询符号之间的调用路径，而 `exact_named_query_nodes`
和 `unique_named_callable_files` 保持命名可调用有用。如果查询
不涉及多态家族，没有任何骨架化。

2. **离开流程主干。** 文件中没有任何符号位于跟踪链上 — 即
链条是代理行走的机制，始终保持满载。

3. **多态兄弟。** 文件的类 `implements`/`extends` 是超类型
具有 **≥ 3 个实施者** (`MIN_SIBLINGS`) — 表明它是众多实施者之一的信号
*可互换*暗示。来自真实的 `implements`/`extends` 边缘，已缓存。

4. **不能幸免。** 如果代理 ** 命名为 a，则文件 ** 幸免**（保持完整）
可调用**——代理要求的命名方法/函数
*参见* (`getResponseWithInterceptorChain`, `SQLCompiler.execute_sql`)，不是
可互换的叶子 — **除非文件本身定义了 ≥3-impl 超类型**。
最后一个子句是覆盖：一个基+子类“family”文件（Django的
`compiler.py`) 是巨大的并且无论如何都要阅读，所以完整的副本只是探索
预算;精简它*释放*兄弟文件代理的预算
否则会阅读。所以： *命名 ⇒ 备用，除非它是家庭档案 ⇒
无论如何都要骨架化。*

完成了两个存储库：

- **`RealInterceptorChain`** — `proceed` 位于书脊上 → 保持完整（条件 2）。
- **`RealCall`** — 离开主干，它通过 **9-impl 触发同级信号
`Lockable` mixin**（不是因为它是可互换的拦截器）。但是
其中代理名为 `getResponseWithInterceptorChain`/`execute`/`enqueue`，并且它
定义没有 ≥3-impl 超类型 → **幸免，保持完整**（条件 4）。这是修复
用于回读：条件之前。 4 它被骨架化，代理将其读回。
- **`BridgeInterceptor` 和其他 4** — 离轴，≥3-impl 兄弟姐妹，仅命名
通过*类型*，定义无超类型→ **骨架化**。胜利。
- **Django `compiler.py`** - 脱离脊柱，一个兄弟（它的子类扩展
`SQLCompiler`)，其中名为 `execute_sql` 的代理 — *但它定义了
`SQLCompiler` 超类型*，因此覆盖触发 → **骨架化**（释放
预算）。相反，放弃它（第一次尝试是错误的）会花费更多，并阅读更多内容。

## 为什么“与 ≥3 个实现者共享超类型”是信号

OkHttp 的拦截器可以互换的原因正是
它们是**一个接口的 N 个实现**，以多态方式调用。那是
图形记录为 `implements`/`extends` 边的*结构*属性：

```
14 classes ──implements──▶ Interceptor      (BridgeInterceptor, CacheInterceptor,
                                              CallServerInterceptor, … )
```

Excalidraw 的 `renderStaticScene`、`Scene`、`Collab` 共享**无**公共
超类型 — ≥3 个实现者查询不会为它们返回任何内容。所以信号
干净地分离两个存储库，并且（在下面验证）留下每个非兄弟存储库
流未受影响。

`≥ 3` 阈值很重要：1:1“服务接口→单个 impl”对（
常见的 Spring/Java 形状）**不是**兄弟姐妹，并且保持完整。只做正品
许多 impl 系列（拦截器链、策略/访问者系列、编解码器
注册中心）迈出大门。

## 骨骼渲染

对于骨架化文件，我们发出类+成员**签名行**（不是
机构）。因为符号节点的 `startLine` 可以指向装饰器/注释
(`@Throws`, `@Override`, `@objc`)，我们向前扫描最多 4 行
实际上“命名”了符号，因此骨架显示了真正的签名：

```
#### …/CallServerInterceptor.kt — CallServerInterceptor, intercept, … · skeleton (signatures only; Read for a full body)
```kotlin
30 对象 CallServerInterceptor : 拦截器 {
32 重写 fun拦截（链：Interceptor.Chain）：响应{
第194章
```
```

标头仍然列出文件的符号并显示 `Read for a full body`，因此
如果代理确实需要的话，可以提取一种特定的实现。

## 验证（细化门）

Headless `claude -p`，Opus 4.8，**有与没有** RustCodeGraph（真正的基准
臂，而不是第一次切割使用的开/关探针）。成本 = 中位数 `total_cost_usd`。

| 回购协议 | 有→无成本 | 与读 | 没有读取 | RealCall/编译器回读 |
|---|---|---|---|---|
| **OkHttp** (n=4) | **0.45 美元 → 0.50 美元**（便宜约 10%） | 2 | 3.5 | **0 / —**（RealCall 已满） |
| **姜戈** (n=6) | **0.56 美元 → 0.63 美元**（便宜约 10%） | 2 | 8.5 | 一半的运行读数为 0 |

两者都是 README 的**成本异常值**（OkHttp 成本高出 3%，Django 成本高出 10%）
成本更高）并且都获得了明显的胜利。 OkHttp WITH 在所有 4 次运行中都更便宜；
Django in 5 of 6（n=6 以了解其高方差）。没有基线匹配
自述文件（0.50 美元/0.63 美元 vs 0.57 美元/0.64 美元），因此增益是WITH-arm 的改进。

**决定性检查现在通过了正确的原因**：使用命名可调用
备用，OkHttp 的 `RealCall` 保持满并且**从不**读回（它是读
在修复之前的 3/4 运行中返回）。惰性存储库 (Excalidraw / Tokio / VS Code /
杜松子酒）保持**0骨架**——通过探针验证——因为精炼的门
骨架化了原始内容的严格子集。 （第一段剪辑是“开与关”，内容如下
平 1 与 3" 声明来自确定性探测查询，并且**不**成立
代理的真实查询——这种不匹配正是这种细化所纠正的。）

## 死胡同（不要重新尝试这些）

1. **降级/排名低价值文件**（例如，扩大 `isLowValuePath` 以删除
`*-testing-support/` 灯具）。提高*内容质量*，但**不提高尺寸** —
探索用其他完整机构补充释放的预算（28,478 → 28,424）。
排名≠缩水；你必须“骨架化”才能缩小。
2. **入口节点成员资格的门。**精确的符号包探索查询*名称*
每个链参与者，所以他们都是“入口节点”——没有分离，什么都没有
骨架化。
3. **依赖接口实现合成器边缘** (`synthesizedBy:'interface-impl'`)
为同级信号。它们**不是**为 OkHttp 的 `Interceptor` 创建的
（Kotlin `fun interface`），所以信号必须来自真实的
`implements`/`extends` 边缘，而不是合成边缘。
4. **一个简单的“核心层”门**（保持第一个N满，骨架化其余）-
骨架化 Excalidraw 的*独特*步骤 → **+17% 成本回归**。这
兄弟姐妹的情况才使其安全。
5. **保留文件，因为它定义了超类型**（第一个细化
试图）。向后：一个基+子类 *family* 文件（Django 的 `compiler.py`，
2,266 行）是巨大的并且无论如何都是可读的，所以保持它完整只是**吃掉 28 KB
探索预算并饿死同级文件**，然后代理读取 - 它
Django 的成本下降到**9%**（0.71 美元）。相反，定义一个超类型
无论如何，让命名族文件骨架化的**覆盖**。
6. **仅使用确定性探针查询验证骨架化。**
探针（`rustcodegraph agent-eval probe-explore <repo> "<symbol bag>"`）和*特工的*真实探索
查询名称符号不同，因此它们形成不同的脊柱并骨架化
不同的文件。探测器说“姜戈：0 具骷髅/读起来平平”；真实的
代理查询骨架化 `compiler.py` 并将其读回。 **始终确认
真实代理 A/B (`run-all.sh`)，而不仅仅是探针。**

## 代码

- `src/mcp/tools.rs`
  - `adaptive_explore_enabled()` — 标志（默认打开）。
  - `format_explore_file_result()` — 收集相关文件，呈现命名符号
流，并应用自适应文件渲染。
  - `format_flow_section()` / `find_named_flow_path()` — 渲染调用路径
命名的查询符号。
  - `explore_family_context()` / `explore_file_mode()` — 骨架化离脊柱
多态同级，同时保持命名或流中文件有用。
- `tests/adaptive_explore_sizing_test.rs` — 7 箱，包括。命名可调用
备用（RealCall）和超类型系列覆盖（compiler.py）。

## 前沿/未来工作

- **族文件中的每个符号骨架化。** `compiler.py` 是
骨架化的整体，所以`SQLCompiler.execute_sql`（基础机构）变成了
签名也是，*是*读回大约一半的 Django 运行。理想是保持
基类的方法完整并仅删除冗余的子类主体 -
缩小有效负载而不忽略答案。全文件骨架化
还不能表达这一点。
- **大的非同级文件主导 Django 的剩余读取。** `query.py` (3,040
线）和 `sql/query.py` 不是多态族，因此骨架化
不能碰它们；代理在 28 KB 集群视图打开时读取它们
不足的。这是探索预算/大文件集群前沿，而不是
骨架化。
- **非接口同级家族**（Go `HandlerFunc` 切片、函数指针
注册中心）不会被捕获——它们没有 `implements`/`extends` 边缘。杜松子酒
例如，中间件链不会触发大门（它的处理程序是 funcs，
不是接口实现）。
- **示例选择**当*没有*拦截器在脊柱上时：今天所有兄弟姐妹
骨架化，代理依赖接口契约；将一个显示为
强制范例可能读起来稍微好一点（未经测试）。
