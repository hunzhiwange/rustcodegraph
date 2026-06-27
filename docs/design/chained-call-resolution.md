# 设计+状态：链式静态工厂/流畅的调用解析

**状态：** 已支持 **13 种语言**（C++、C、PHP、Java、Kotlin、C#、Swift、Rust、
Go、Scala、Dart、Objective-C、Pascal/Delphi）+ 一致性检查。 **TypeScript 和 Luau
被评估并故意跳过**（都逐渐键入→机制为+0 /
回归真实代码）。请参阅下面的“完整自述文件分类”。追踪问题：
**#750** （最初是“静态类型自述语言”，但该枚举是
不完整——它错过了 ObjC / Pascal / Luau）。

**动机：**一个调用，其**接收者本身就是一个调用** - 工厂/单例/
返回对象的构建器 - 应该为链接方法生成 `calls` 边：

```java
Foo.getInstance().bar();   // bar() should resolve to Foo::bar, never a same-named decoy
```

在这项工作之前，每种静态类型语言都**放弃了接收器**并且
名称与裸方法 (`bar`) 匹配，因此在 9 种语言中的 7 种中，它默默地附加到
**不相关类型上的同名方法** - 正确性错误，而不仅仅是缺少覆盖范围。

---

## 三部分机制（每种语言）

1. **捕获工厂声明的返回类型** - 每种语言的 `getReturnType`
钩子写入 `nodes.return_type`（架构 v5）。 `*Foo`→`Foo`，`List<Bar>`→`List`，
`pkg.Foo`→`Foo`、`-> Self` / `: self` / `this.type` → 声明类型。
2. **在提取时保留链式接收器** — `tree-sitter.ts`（或定制
extractor) 将 `Foo.getInstance().bar()` 编码为标记字符串
`Foo.getInstance().bar`（`().` 标记永远不会出现在普通参考中）。一个
每个语言的门保持**实例**链（`list.map().filter()`）裸露，以便它们
现有的分辨率保持不变——只有大写接收器/工厂链重新编码。
3. **解析并验证** — 在解析时，接收器的类型是根据什么推断出来的
内部调用返回，然后外部方法在该类型上解析**并且
已验证：该方法必须存在于该类型（或其符合的超类型）上，因此
错误的推论不会产生**任何优势**，而绝不会产生错误的推论。

`src/resolution/name-matcher.ts` 中的三个共享解析器，全部调用
`resolveMethodOnType`（具有一致性超类型-walk）：

| 旋转变压器 | 接收器样式 | 语言 |
|---|---|---|
| `matchCppCallChain` | `field_expression` (`Foo::instance().bar`) | C++、C |
| `matchScopedCallChain` | `::`（`Cls::for($x)->m`、`Foo::new().bar`） | PHP、铁锈 |
| `matchDottedCallChain` | `.` (`Foo.create().bar`) | Java、Kotlin、C#、Swift、Go、Scala、Dart |

**一致性通过 (#754)。** 当链接方法存在于 **超类型** 时
返回类型符合（继承/默认接口/特征/混合/嵌入
方法），第一遍看不到它 - `implements`/`extends` 边尚未构建。
因此失败的链引用被推迟（`resolution/index.ts` 中的 `CHAIN_LANGUAGES`）并且
边缘存在后，在第二遍 `resolveChainedCallsViaConformance()` 中重新解析，
步行`context.getSupertypes(...)`。

**添加语言：** `languages/*.ts`中的`getReturnType`；对链接的接收器进行编码
+ 节点型门；将语言添加到右侧 `matchReference` 门（并且
`CONSTRUCTS_VIA_BARE_CALL`（如果是裸露的大写调用构造该类）；添加
`CHAIN_LANGUAGES`;综合测试 + 真实仓库 A/B；凹凸`EXTRACTION_VERSION`。

---

## 覆盖范围（均通过合成诱饵/缺失方法测试 + 真实回购 A/B 进行验证）

| 语言 | 公关 | 接收者 | Real-repo A/B（独特的 `calls` 边） | 笔记 |
|---|---|---|---|---|
| **C++ / C** | 第645章 （742） | `field_expression` | — | 原始：单例/工厂/链式吸气剂。 |
| **PHP** | 第608章（749） | `::` → `->` | — | `Cls::for($x)->method()` — Laravel 每租户客户端习惯用法。 `: self`/`: static`。 |
| **Java** | 第751章 | `.` | 番石榴 **+1,507 / −0** | 缺边 → 纯累加。 |
| **科特林** | 第752章 | `.` | 箭头 **+49 / −438** | 错误边缘 → 精度获胜（删除 438 = 测试/文档噪声 + 错误）。需要大写接收器门+构造器接收器处理。 |
| **C#** | 第753章 | `.` | Newtonsoft +3 / NodaTime **+73 / −0** | 添加剂。返回类型为`returns`字段；扩展方法链无法正确解析。 |
| **一致性** | 第754章 | （解析器升级） | 箭头 **+22 / −0** | Supertype walk — 支持 Swift 协议扩展、Rust 特性、Go 嵌入、Dart mixin、Java/Kotlin/C# 继承链。 |
| **迅速** | 第755章 | `.` | 阿拉莫菲尔 / 翠鸟 **0 / 0** | 中立安全（唯一流畅的名称已经裸解析）。需要嵌套扩展命名修复（`KF.Builder`→`KF::Builder`）。 |
| **锈** | 第757章 | `::` | 拍手 **+937 / −775** | 精准获胜（622 错误→正确重定向，+162 净值）。 `-> Self`;通过一致性的特征默认方法。单跳。 |
| **去** | 第760章 | `.` | 杜松子酒 **净零** | `New().Method()`;通过一致性嵌入结构。可变内部后备。 **发现+修复了批处理解析器失控**（变异的`original.referenceName`循环了偏移0批次→5M边缘/1.4GB；通过将回退与原始引用绑定+非进度防护来修复）。 |
| **斯卡拉** | 第761章 | `.` | 加特林 **+14 / −59** | 精度获胜（−59 = stdlib `Option`/`Iterator` `.map`/`.flatMap` 基线错误地连接到加特林的 `Validation::*`）。同伴工厂+案例级`apply`。 |
| **镖** | 第762章 | `.` | localsend 手写 **+17 / −10** | 精确获胜 **+ 构造函数成为一流**（工厂/命名构造函数 `Foo.create()`/`Foo._()` 现在已编入索引；未命名的 `Foo()` 保留为 `instantiates`）。 `dartCtorInfo` 根据封闭的类名验证 ctor - 处理树保护错误解析，其中 `@override (A,B) m()` 使 `m()` 看起来像一个 ctor。 |
| **Objective-C** | 第786章 | 消息发送 | SDWebImage **+35 / −75** | 精准致胜。链接消息通过 `message_expression` 发送 `[[Foo create] doIt]`。 getReturnType 跳过可为空限定符 (`nonnull instancetype`)。类消息工厂按约定返回接收者类，因此 `[[X alloc] init]` / 单例链在 `X` 上解析（已验证）。 −75 是错误的 `init` 错误匹配，被重定向到正确的类别。 |
| **帕斯卡/德尔福** | 第791章 | `.` (`exprDot`) | 帕斯卡币 **+19 / −18** | 精准致胜。 `TFoo.GetInstance().DoIt()` 优于 Pascal 的 `exprCall`/`exprDot`。从 `typeref` 获取返回类型（包括接口返回 `IFoo`）。根据 Delphi `TFoo`/`IFoo` 类型约定重新编码，因此大写的*变量*链保持裸露。构造函数（无 `: TBar`）或类型转换 `TFoo(x)` 在类上解析。 -18 个中的 15 个是正确的类→接口重定向 (`GetInstance(): IAsn1OctetString`)。 |
| **打字稿** | — | `.` | typeorm +0/−6 · 嵌套 **+0/−164** | **已评估，未发货** — 逐步打字；见下文。 |
| **卢奥** | — | `:` / `.` | 融合+0/−0·物质+0/−0 | **已评估，未发货** — 逐步打字；添加剂安全（缺边间隙，无回归），但真正的 Luau 很少注释工厂回报，因此两个基准均为 +0。适用于 `Foo.create(): Bar`，然后适用于 `:doIt()`（合成）。 |

`EXTRACTION_VERSION` 现在是 **18** （C++→…→Pascal 链→无父母调用→自由例程归因）。使用 `rustcodegraph index -f` 重新索引
在现有图表上选择较新的提取器。

## 为什么 TypeScript 被跳过

该机制从工厂的**声明的**返回类型中解析出一条链。打字稿
依赖于**类型推断**——例如NestJS 的 `Test.createTestingModule(m) { return new
TestingModuleBuilder(...) }` has no `:TestingModuleBuilder` 注释 — 所以
工厂的类型无法恢复，重新编码的链无法解析，并且它**丢弃
现有解析器找到的裸名边缘**。 Real-repo A/B 都添加了 **+0
typeorm 和 Nest** 具有净召回率回归（nest 上为 −164，主要是无处不在的
`Test.createTestingModule({…}).compile()` 图案）。去除的边缘大部分是
*错误*（基线错误地将 `.compile()` 解析为 `ModuleCompiler::compile`），所以它是
精度为正但召回为负 — 反对召回优先不变量，并添加
没有什么不伤害的地方（TS 方法名称足够独特，裸名称已经
让他们着陆）。它已完全实施（通过了 5 项综合测试，安全失控的裸名
后备）并有意识地不发货。获得 TS 胜利的唯一途径是阅读
**推断**返回类型（在工厂主体中解析 `return new X()`） - 非常多
较大的变化。关于问题 #750 的完整文章。

---

## 完整的自述文件分类（所有 21 种语言）

该机制的真正要求是**声明的返回类型**来恢复接收者的
type — 不是“静态类型”（PHP 通过其 `: self` / `: Type` 返回进行限定
声明）。根据自述文件的完整支持语言列表：

| 桶 | 语言 |
|---|---|
| **涵盖** (13) | C++、C、PHP、Java、Kotlin、C#、Swift、Rust、Go、Scala、Dart、Objective-C、Pascal/Delphi |
| **已评估，已跳过** (2) | **TypeScript** — 渐进式打字 → 推理类型工厂无法恢复；净回忆回归。 **Luau** — 逐渐打字；添加剂安全，但 Fusion AND Matter 为 +0（真正的 Luau 很少标注工厂退货）。两者：该机制需要可靠地声明返回类型，而逐渐类型化的代码经常会忽略这一点。 |
| **帕斯卡呼叫覆盖后续行动** | 链式调用工作中的两个差距都已解决。 **无括号调用 (#793)：** Pascal 允许无参数方法删除其括号 (`Obj.Free;`、`TFoo.GetInstance.DoIt;`)，这些括号被解析为裸 `exprDot` 并且根本不会提取为调用。现在提取，作用域为 STATEMENT 位置（赋值/条件位置中的裸点被单独保留 - 与字段/属性访问不明确）。 PascalCoin A/B **+1131 / −1**，所有新边都解析为方法。 **自由例程归因（#795）：**仅在 `implementation` 部分定义的过程/函数（没有接口 decl，不是方法）没有节点，因此其主体的调用集中在文件下；现在它得到一个函数节点及其调用属性。 PascalCoin A/B **+511 / −145** （文件级聚合 → 每个例程边缘）。 |
| **超出范围 - 未声明返回类型** (6) | JavaScript、Ruby、Lua、Svelte、Vue、Liquid（Liquid 根本没有方法/链） |
| **部分/单独** (1) | Python — 仅可选 `-> T` 提示；追踪为#578，不属于该机制的一部分 |

所以 #750 的原始框架（“9 种静态类型自述语言”）是不完整的 —
它错过了另外三种类型的语言，现已全部解决：** Objective-C ** 已发布（#786，
相同的错边间隙，机构端口直接）； **Pascal/Delphi** 已发布（#791，一个干净的
配对链的端口 - 最初的“阻塞”读取是错误的，仅由探测引起
无括号形式）； **Luau** 评估并跳过（逐步输入→真实存储库上的 +0，
添加剂安全）。

直通车：此机制适合具有**可靠声明的返回类型**的语言
（13 已发货）。渐进式语言（TypeScript、Luau）经常忽略它们
它是有回报的，而动态类型语言却没有。

---

## 边缘案例/模型
- **单跳**：一条链重新编码一跳；更深的跳跃（`a.b().c().d()`）保持
裸名称（内部 `()` 击败了 `Class::method` 拆分）。重新测量深度
flutter-builder 仓库。
- **验证，而不是猜测**：每个解析器都以 `resolveMethodOnType` 结尾，因此
未知/错误的推断类型产生**无边缘** - 诱饵/缺席方法
保证可以安全运输。
- **每种语言接收器门**保持实例链裸露，因此现有分辨率是
从未退步； A/B“删除”计数是错误边缘修正，而不是损失。

## 相关工作
- **动态调度/回调综合**（一个*不同的*机制）：观察者/
EventEmitter / React-render / JSX-child / django-ORM 边缘合成住在
`callback-edge-synthesis.md` + `dynamic-dispatch-coverage-playbook.md`。
- #750 的详细会话工作说明位于
`.claude/handoffs/chained-call-multilang-probe.md`（草稿；该文档是
永久记录）。
