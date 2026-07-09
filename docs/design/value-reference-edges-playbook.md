# 剧本：将价值参考边缘扩展到新语言

**目的。** 这是用于添加+验证值引用边缘的操作手册
覆盖另一种语言。将新会话指向此文件并说 **“开始于
语言 X"** — 它拥有一切：功能如何工作、代码在哪里、确切的
验证配方（带有脚本）、每种语言的清单以及已经遇到的陷阱。

设计原理和验证矩阵已在配套文档中完成：
[值引用边设计文档](./value-reference-edges.md)。该文件是*操作方法*。

---

## 0.“从 X 语言开始”——按顺序执行此操作

1. 阅读第 1 节（工作原理）和第 2 节（当前状态），以便您了解其机制和所做的事情。
2. 进行**每种语言的接线检查**（第 5 步 A-C）——这是语言不同的地方，
大多数实际工作/决策都在哪里。不要跳过：错误的声明符节点类型或
类范围与文件范围不匹配使得该功能默默地不发出任何内容（或错误的边缘）。
3. 在小型/中型/大型 **公共 OSS** 存储库上运行 **验证扫描** (§4)
语言。狩猎 FP。 **修复FP集群；记录单例。**（参见§3了解什么是真正的 FP
看起来像是一个可以接受的。）
4. 在 `value-reference-edges.md` 中向矩阵添加一行**，并在
`__tests__/value-reference-edges.test.ts`。
5. 提交一个分支，打开一个 PR。 （§6 有 git 工作流程 + 之前的 PR 是如何完成的。）

范围规则（硬）：**永远不要对维护者自己的存储库进行评估** - 克隆真正的公共 OSS
该语言的存储库。 （内存：`agent-eval-targets-public-oss-only`。）

---

## 1. 值参考边如何工作

**什么：** `references` 边缘与 `metadata: { valueRef: true }` 从*读卡器符号*到
**它读取的文件范围 `const`/`var`**，仅限同一文件。它存在所以影响分析
捕获“更改此常量/配置对象/查找表→影响其读者”——一个类
从未捕获的更改调用/导入/继承边缘（const 的消费者过去常看
就像“没有什么取决于这个”）。

**流向：** 直接进入 `getImpactRadius` → `rustcodegraph impact` 和撞击轨迹
在 `rustcodegraph_explore` / `rustcodegraph_node`。无需改变代理行为。 **胜利是
影响半径正确性**（常量 90 个符号从“1受影响”读取到“90”），*不是*
代理读取减少（参见§4.3）。

**代码 — 全部在 `src/extraction/tree-sitter.ts` 中：**

| 象征 | 角色 |
|---|---|
| `VALUE_REF_LANGS`（静态集） | 该功能运行的语言。目前`typescript`、`javascript`、`tsx`、`go`、`python`、`rust`、`ruby`、`c`、`java`、`csharp`、`php`、`scala`、`kotlin`、`swift`、 `dart`、`pascal`。 **在此处添加新语言。** |
| `valueRefsEnabled` | `process.env.RUSTCODEGRAPH_VALUE_REFS !== '0'` — 默认开启，env 选择退出。 |
| `MAX_VALUE_REF_NODES` (20_000) | 每个范围的遍历上限（以及阴影扫描上限）。 |
| `captureValueRefScope(kind, name, id, node)` | 从每个节点上的 `createNode` 调用。记录**目标**（文件范围 `const`/`var`）和**读取器范围**（`function`/`method`/`const`/`var`）。 |
| `flushValueRefs()` | 在 `extract()` 末尾调用一次。修剪阴影目标，然后对于每个读取器范围，遍历其子树以查找与目标名称匹配的标识符并发出边缘。 |

**`captureValueRefScope` 内的两个门**（您可能需要根据语言进行调整）：

- **目标门：** `kind ∈ {constant, variable}` **和** `name.length >= 3` **和**
`/[A-Z_]/.test(name)`（独特的名称 - 避免单字母/全小写阴影）
**和** 节点的父 ID 以 `file:`、`class:` 或 `module:`（文件/类/模块范围）开头。
- **读卡器门：** `kind ∈ {function, method, constant, variable}`。

** `flushValueRefs` 中的发射循环：** 仅同一文件（目标 + 范围是每个文件，重置
每次冲水）；按 `(reader, target)` 进行重复数据删除；跳过 `isGeneratedFile(path)`； **李子阴影
目标**（参见§3）。

---

## 2. 当前状态（已发货+已验证）

- **对于 TS/JS/tsx + Go + Python + Rust + Ruby + C + Java + C# 默认开启**（`RUSTCODEGRAPH_VALUE_REFS=0` 禁用）。已发货 **PR #895**
（翻转+阴影修剪）； Go 在稍后的 PR 中添加（shadow-prune 声明符开关 +
`VALUE_REF_LANGS`）； C 稍后添加（提取器更改为发出节点+裸标识符
误解析守卫）；之后是 Java + C#（字段 → const 子集的常量类型切换）。
- **在 **TS、JS、tsx、Go、Python、Rust、Ruby、C、Java 和 C#** 中验证了 S/M/L** — 请参阅中的矩阵
设计文档。一切干净：节点数相同开/关，精确防护，冲击获胜
转载。 Go 需要扩展影子修剪（每个语法声明符）——有效的
“步骤 B 是承重”的示例。 **C 需要 Ruby 处理**（提取器没有发出
C 文件范围 const/var 节点）**加上** C 特定的 FP 保护（宏前缀原型
错误解析会创建一个以返回类型命名的裸标识符“变量”——跳过 bare-`identifier`
声明者）。这是“§2b 覆盖表的 *easy-path* 猜测可以是
错误——在信任它之前总是执行§5步骤C（确认节点存在）。”
- **Java + C# 是最干净的类范围（“Ruby 处理”）语言。** 常量已经
提取物——但是是 `field` 类型，被门拒绝。整个变化正在发出 const
*子集* as `constant`：每个提取器上的 `isConst` 谓词（Java `static final`；C# `const`
/ `static readonly`) + `extractField` 中的一种开关。 **没有新的阴影修剪布线**（方法
本地人是 `variable_declarator`，已经在交换机中）并且**没有 FP 防护**（UPPER_SNAKE /
PascalCase 适合独特名称门）。实例 `final`/`readonly` 字段正确保留
`field`。已验证的 S/M/L：gson/commons-lang/guava、automapper/newtonsoft/efcore — 0 泄漏、节点
平价，大影响获胜（`INDEX_NOT_FOUND` 4→165，`_resourceManager` 22→1664）。
- **PHP 是最干净的 — 一条读者扫描线。** 常量已提取为 `constant`
（顶级 + 类），因此唯一的变化是教读者扫描 PHP 常量
*reference* 是一个 `name` 节点（裸 `X`，或 `self::X` / `Foo::X` 的 const 一半）。 **无提取器
更改，无需修剪接线**（`$var` 本地不能隐藏裸常量 - 不同的命名空间）。
经过验证的 S/M/L (guzzle/monolog/laravel)，全部干净，0 类/const 冲突。诚实的警告：
**产量较低** — PHP 跨文件读取常量远多于同一文件读取常量（laravel 2,956 个文件→ 86
边缘），并且 value-refs 仅限于同一文件；仍然正确，只是贡献较小。
- **Scala — `object` 是常量范围。** Scala 没有 `static`；单例 `object` 的 `val`
是共享常量习惯用法 (`object Config { val Timeout = 30 }`)。顶级`val`已经
提取为 `constant`，但对象/类值都以 `field` 形式出现。修复：在 Scala 中
`val_definition` 处理程序，走到封闭的定义 - `object_definition` （或顶级）→
`constant`/`variable`； `class`/`trait`/`enum` → `field`（每个实例，如 Java 实例 `final`）。
将 `val_definition`/`var_definition` 添加到阴影修剪（方法本地 `val` 阴影）。读者扫描
不需要任何东西（参考文献是`identifier`）。次要已知限制：Scala 使用 `val`/`def`
对于成员来说可以互换，因此驼峰命名法的 val 可以与方法共享一个名称 - 相同的文件名
匹配无法区分它们（有界，就像 Ruby 的兄弟类；扫描显示标记的碰撞
大部分是兄弟姐妹读取的真实对象值）。经过验证的 S/M/L（upickle/cats/pekko）。
- **尝试并恢复了 C++ — 在未首先解决解析保真度的情况下不要重试。** tree-sitter-cpp
错误解析真实模板/宏重的 C++（和 `.h` 文件路由到 C 语法）：类成员和
参数作为虚假常量/变量泄漏到文件范围。两个守卫（跳过`ERROR`-祖先和
`compound_statement`-祖先声明）消除了约 83% 的总泄漏，但残留物普遍存在
甚至是结构良好的库源（模板类成员泄漏、合并的大型标头、
`.h`-as-C++)。它没有达到其他语言的精度标准。请参阅下面的 C++ 部分。
- **Kotlin = C + Scala + PHP 技术组合（并且干净）。** 之前没有提取任何内容（属性名称
嵌套 `property_declaration → variable_declaration → simple_identifier` — C 问题）。使固定：
在 Kotlin `visitNode` 钩子中处理 `property_declaration` — 拉出嵌套名称，走到
包含该类型的定义（`object`/`companion object`/顶级 → `constant`/`variable`；
`class` → `field` — Scala 规则；跳过 `function_body`/`init`/lambda 下的局部变量），添加
`simple_identifier` 到读者扫描（PHP-`name` 移动），`property_declaration` 到
阴影修剪。干净的解析保真度（已经处理了一个 `fun interface` 错误解析），所以没有
C++ 风格的尾部。最干净的收益之一——伴生对象位掩码/状态常量是一个沉重的负担
相同文件读取习惯用法。已验证的 S/M/L (okio/coroutines/ktor)；仅有界 val/def-or-class 和
兄弟伙伴名称重叠仍然存在（与 Scala/Ruby 共享）。
- **Swift 重用了 Kotlin + 两个 Swift 特有的功能。** 类型中的顶级 `let` + `static let` 是
共享常量（`enum`/`struct` 命名空间）；实例 `let` 保持 `field`。嵌套名称
(`property_declaration → <name> pattern → simple_identifier`);读者扫描已涵盖
（`simple_identifier`，来自 Kotlin）。两个新东西：**（1）目标门被拓宽到`struct:`/
`enum:` 父母** — Swift 命名空间常量 (`enum Constants { static let X }`)，以及每个
其他语言的目标是`file:`/`class:`/`module:`； **(2) 计算属性被跳过**（a
`var x:Int{ … }` getter 没有存储值 — 检测 `computed_property` 子项）。节点创建
插入*现有* Swift `property_declaration` 处理程序（属性包装器/类型依赖），留下
没有动过的。干净的解析，没有尾巴。已验证的 S/M/L（Alamofire/swift-argument-parser/swift-nio）。
- **Dart — 干净的语法分离，但修复了同级阅读器扫描问题。** Dart 的语法已经存在
分割情况： **`static_final_declaration`** *确切地*是顶级/`static` `const`/`final`
（共享常量习惯用法），而实例字段/`var` 使用 `initialized_identifier` 且局部变量使用
`initialized_variable_definition` — 因此提取 `static_final_declaration` → `constant` （在
`visitNode` 钩子）**没有需要保护的实例/本地泄漏**。读者免费扫描（Dart 参考文献是
`identifier`）。问题是 **读者扫描**：Dart 附加一个方法/函数 `body` 作为 *next
签名节点（存储范围）的兄弟*，而不是子节点，因此扫描仅看到签名
**什么也没发现**，直到它被教导拉入一个 `function_body` 的下一个兄弟姐妹（仅在其中飞镖）
值参考集）。需要阴影修剪 `static_final_declaration` + `initialized_identifier` +
`initialized_variable_definition`（本地 `const X` 隐藏文件 `const X`）。已验证 S/M/L
（http/flame/flutter-packages）。 **警告：**生成的 Dart 文件会加剧同级类的歧义
（具有数百个 `static final _class` 的 JNIGEN `_bindings.dart` 折叠为文件范围目标）。
常见的代码生成后缀（`.g.dart`/`.freezed.dart`/`.pb.dart`）已被过滤
`isGeneratedFile`;仅标头标记的生成器（JNIGEN）不是，所以真正的源是干净的，但是
生成的 FFI/JNI 绑定有噪音。
- **Pascal — 真正的简单路径 + Dart 兄弟体再次修复。** 单元/类 `const` *已经*
提取为 `constant` (`variableTypes: ['declConst', …]`)，因此它被添加到-`VALUE_REF_LANGS` +
阴影修剪（`declConst`/`declVar`；本地 `const X` 阴影单元 `const X`）。问题是
与 Dart *相同的*阅读器扫描错误：Pascal 的 proc 主体是 `declProc` 的 **`block` 兄弟**
header（阅读器范围），都在 `defProc` 下 - 因此相同的兄弟拉修复被扩展到
`block`。阅读器扫描节点类型已涵盖（参考文献为 `identifier`）。 **低产量**——帕斯卡读到
常量跨单元多于同一文件（马：4 条边）。 **警告：** Pascal 不区分大小写，
但读者扫描与确切的文本匹配，因此错过了不同大小写的参考（没有 FP，只是一个
错过）;不值得正常化。
- **测试：** `__tests__/value-reference-edges.test.ts` — 同文件阅读器边缘；出现在
冲击半径； Shadowed const NOT Edged（已验证在没有防护的情况下会失败）；仅 JSX 读取
边缘（tsx）； `RUSTCODEGRAPH_VALUE_REFS=0` 不发出任何信号。
- **内存：** `value-reference-edges-default-on`（A/B发现+影子守卫原理）。

---

## 2b.覆盖范围与自述文件（语言+框架）

根据自述文件的 **支持的语言** 表（24 行）和 **框架感知进行跟踪
路线**列表。值引用是**语言级别**，因此框架“不是”一个单独的轴（请参阅
本节底部）。

**✅ 完成 — 验证 S/M/L（15 + 3 继承）：**

| Language | How |
|---|---|
| TypeScript, JavaScript, tsx | 文件范围 `const`/`var`；原始语言 |
| Python | 模块级 `NAME =` |
| Go | 封装 `const`/`var` |
| Rust | 模块 + 实现 `const`/`static` |
| Ruby | 类/模块 `CONST`（类范围扩展） |
| C | 文件范围 `static const` 标量 + 指针/数组查找表 + 可变全局变量。 **需要更改提取器**（未发出节点）+一个裸标识符误解析防护 - 不是下表首先猜测的简单路径 |
| Java | 类 `static final` 字段。节点以 `field` 类型存在；将 const 子集发出为 `constant` （`isConst` + `extractField` 类型开关）。没有新的修剪布线，没有 FP 防护装置 |
| C# | `const` / `static readonly` 类。与Java相同——相同的`field`→`constant`变化 |
| PHP | 顶级 `const` + 类 `const` （两者都已经是 `constant` 类型）。 **唯一**的变化是读者扫描：PHP const *引用*是一个 `name` 节点。无需更改提取器，无需修剪接线（`$var` 局部不能隐藏裸常量）。产量较低——PHP 跨文件读取 const 的次数多于同一文件读取的次数 |
| Scala | 顶级 `val` （已经是 `constant`）+ **`object` val** （单例常量惯用语；通过步行到封闭的 `object_definition` 从 `field` 重新启动）。 `class`/`trait`/`enum` 仍为 `field`。 `val_definition`/`var_definition` 添加到阴影修剪中。次要 val/def 名称冲突限制 |
| Kotlin | 顶级 / `object` / `companion object` `val` （从无到有重新排序 - 属性根本没有提取）。在 `visitNode` 中处理：嵌套名称（`variable_declaration → simple_identifier`，C 移动）+ kind 的作用域遍历（Scala 移动）+ 阅读器扫描中的 `simple_identifier`（PHP 移动）+ 修剪。 `class` 实例值仍为 `field`。 Clean——最好的产量之一（配套位掩码） |
| Swift | `struct`/`enum`/`class` 中的顶级 `let` + `static let`。重用 Kotlin（嵌套名称 + `simple_identifier` 读者扫描）。两个 Swift 接触：**门扩大到 `struct:`/`enum:` 父级**（那里有 Swift 命名空间常量），以及**跳过计算属性**。 `class`/实例存储的道具保持`field`。插入现有 Swift 属性包装处理程序 |
| Dart | 顶级 `const`/`final` + 类 `static const`/`static final` — 所有 **`static_final_declaration`** 节点，通过语法与 instance/`var`/local 完全分离（因此没有泄漏保护）。 `visitNode` → `constant`。需要读取器扫描修复：Dart 的方法 **body 是签名的下一个同级**，因此扫描会拉入 `function_body` 同级。生成的 FFI 噪声 (JNIGEN `_bindings.dart`) 是一个需要注意的事项 |
| Pascal / Delphi | 单位/类别 `const`（已提取为 `constant`）。添加到 `VALUE_REF_LANGS` + 阴影修剪 (`declConst`/`declVar`) + **相同​​的 Dart 兄弟主体修复**（Pascal 的 proc 主体是 `declProc` 标头的 `block` 兄弟）。产量低（跨单元读取）；不区分大小写（精确文本扫描会错过重新大小写的引用） |
| **Svelte, Vue, Astro** | **免费继承** - 他们的提取器将 `<script>`/frontmatter 块重新解析为 `typescript`/`javascript`，它们位于 `VALUE_REF_LANGS` 中（已验证：`.svelte` `const` 边缘其读者）。没有单独的工作；不需要单独的矩阵行。 |

**🔜 剩余 — 可能是简单的路径**（常量是文件/模块范围，或顶级；执行 §5：添加
到 `VALUE_REF_LANGS`，验证声明符节点类型 + 提取器类型，扫描）。分别分类
*在*构建之前——有几个是混合文件+类范围。 **从 C:** 此处学到的警告“简单路径”
意味着 *scope* 适合——它不保证提取器已经发出 const 节点。 C在这个
列，但发出 *没有* 文件范围 const/var 节点（其名称嵌套在 `init_declarator` 中）
通用回退无法读取），所以它毕竟需要 Ruby 风格的提取器更改。 **始终运行
§5 步骤 C（确认 `select kind,name from nodes …` 实际上显示常量），然后再信任此
柱子。**

| Language | Constant forms | Note |
|---|---|---|
| Lua / Luau | 文件/块 `local X =` + 全局变量；没有 `const` 关键字 | 独特名称门（需要 `[A-Z_]`）捕获较少 - Lua 大小写不同 |
| R | 文件范围 `X <- …` / `X = …` | |

**🧱 剩余 — 需要 Ruby 处理**（常量几乎完全存在于 **
类别/类型**；类范围 *gate* 现在存在，但首先确认提取器将它们发出为
`constant`/`variable` 节点 — Ruby 的节点根本没有被提取，并且类字段通常会出现为
`field`/`property`类，门拒绝）。 **Java + C#（完成）是这种情况**：他们的
常量提取为 `field` 类型，修复方法是发出 const 子集 (`static final` /
`const` / `static readonly`) 作为 `constant` — 此存储桶其余部分的模板：

| Language | Constant forms |
|---|---|
| Objective-C | `static const` / `extern const` / `#define` （文件式；宏未解析；已经“部分支持”） |

**⛔ 尝试并恢复 — C++。** 文件范围 + 类 `static const`/`constexpr`（混合）。机械
在干净的 C++ 上构建并正确，但是 **tree-sitter-cpp 解析保真度是障碍**： template/
宏重的真实 C++ 将类成员 + 参数作为虚假常量/变量泄漏到文件范围，并且
`.h` 文件路由到 C 语法（修改 C++ 类）。两个守卫（跳过`ERROR`-祖先和
`compound_statement`-祖先声明）减少了约 83% 的总泄漏，但残留物甚至遍布
结构良好的库源。 **没有达到精度吧；已恢复。** 不要重试
“value-refs”任务——它需要事先进行 C++ 解析处理工作（模板类成员作用域、
`.h`-as-C++ 检测，合并标头排除）。

**🚫 N/A：** Liquid（模板语言 - 没有要跟踪的值常量）。

**框架 — 不是值引用轴。** 自述文件的框架列表（Django、Flask、Express、
NestJS、Rails、Spring、Gin、Laravel 等）是一个*单独的*功能：**路由节点提取**。
Value-refs 与框架无关——它通过以下方式覆盖任何框架代码中的常量
底层语言支持，**每个框架无需执行任何操作**。验证已经完成
在框架存储库上运行（Rails → Ruby、Django → Python、gin → Go、express/eslint/webpack → JS，
jekyll/sinatra → Ruby)，因此框架代码得到了运用；没有单独的框架矩阵。

---

## 3. 精准卫士 + 什么算作误报

守卫在 `flushValueRefs` 中运行，顺序为：

1. **`isGeneratedFile(path)`** (`src/extraction/generated-detection.ts`) — 跳过
*后缀识别*生成的文件（`.pb.ts`、`.min.js`，...）。 **仅路径** - 无法捕获
内容精简的捆绑包。
2. **Shadow prune** — 当目标的 **声明符计数超过其文件范围节点时删除目标
count** （因此它也绑定在内部/本地范围内）。基本原理：捆绑/Emscripten `const
Module` re-declared as an inner `var Module`, a Go package const shadowed by a local `:=`，或
由本地 `=` 遮蔽的 Python 模块 const 解析为嵌套的 *inner* 绑定
读者，因此文件范围边缘是错误的。内部重新绑定不是图形节点，因此声明符
在**语法树**级别进行计数。 *这是每种语言敏感的防护：*
声明符节点类型因语法而异（§5 步骤 B），并与文件范围节点进行比较
count（不是平坦的 `>1`）是保持**条件模块定义**（`try: X=…; except: X=…`）的原因。
3. **独特名称+相同文件**（目标门）。

**真正的 FP 是什么样子**（修复它）：读者边缘到文件范围 const 它**不**
实际读取 - 几乎总是**文件内阴影**（名称重新绑定在内部
范围）集中在**捆绑/缩小/生成**文件中。在 exlidraw 上这是 23 条边
在一个 Emscripten 斑点中。

**什么不是 FP**（保留它）：
- **CommonJS `var x = require('…')` 绑定** (JS) — 正确的同一文件读取；改变
绑定*确实*会影响其读者；针对 `calls` 影响的边缘进行重复数据删除。不是噪音。
- **模块级可变 `var` 状态** 由许多同文件函数读取 - 预期的情况。
- 如果精度保持不变，语言中较高的边缘份额（JS ~4–5% vs TS ~0.7–1.6%）就很好。

**已知限制（有意为之，已记录）：**仅参数阴影*不受*保护
（修剪计算声明符，而不是参数 - 保护它会过度修剪合法的常量，其
名称与参数一致）；仅同一文件（无跨文件使用者）；反应式/计算式
不包括没有静态标识符的读取。

---

## 4. 验证配方

### 4.1 确定性探测（核心——寻找FP）

对同一个存储库进行两次索引（在 vs `RUSTCODEGRAPH_VALUE_REFS=0` 上）；节点数**必须相同**
（仅边缘特征）。首先构建：`npm run build`。将其另存为 `probe.sh`：

```bash
#!/usr/bin/env bash
set -uo pipefail
SRC="$1"; NAME="$2"; WORK="${WORK:-/tmp/cg-vr}"
CG="${CODEGRAPH_BIN:-$(pwd)/target/release/rustcodegraph}"
export RUSTCODEGRAPH_TELEMETRY=0 DO_NOT_TRACK=1 RUSTCODEGRAPH_NO_DAEMON=1
ON="$WORK/$NAME-on"; OFF="$WORK/$NAME-off"
rm -rf "$ON" "$OFF"; mkdir -p "$WORK"
rsync -a --exclude='.git' "$SRC/" "$ON/"; rsync -a --exclude='.git' "$SRC/" "$OFF/"
"$CG" init "$ON"  2>&1 | grep -E "nodes,|Indexed"
RUSTCODEGRAPH_VALUE_REFS=0 "$CG" init "$OFF" 2>&1 | grep -E "nodes,|Indexed"
OND="$ON/.rustcodegraph/rustcodegraph.db"; OFD="$OFF/.rustcodegraph/rustcodegraph.db"
echo "nodes on/off: $(sqlite3 "$OND" 'select count(*) from nodes') / $(sqlite3 "$OFD" 'select count(*) from nodes')  (MUST MATCH)"
# PRECISE filter — do NOT use LIKE '%valueRef%' (it matches filenames like
# textModelValueReference.ts; see §7). Always: kind='references' AND the exact key.
F="kind='references' and metadata like '%\"valueRef\":true%'"
echo "value-ref edges: $(sqlite3 "$OND" "select count(*) from edges where $F")"
echo "=== top targets by same-file reader count ==="
sqlite3 -column "$OND" "select t.name, count(*) r, replace(t.file_path,'$ON/','') f from edges e join nodes t on e.target=t.id where e.$F group by e.target order by r desc limit 15;"
```

运行：`WORK=/tmp/cg-vr bash probe.sh /path/to/cloned-repo reponame`。

### 4.2 FP 搜索（针对 ON db `$OND` 运行，上面使用 `F`）

```bash
# (a) bundled/minified files among targets — the #1 FP source (the woff2 case):
sqlite3 "$OND" "select distinct t.file_path from edges e join nodes t on e.target=t.id where e.$F;" \
 | while read -r f; do [ -f "$f" ] || continue; \
     m=$(awk '{if(length>x)x=length}END{print x+0}' "$f"); [ "$m" -gt 300 ] && echo "MINIFIED? $m $f"; done
# (b) guard invariant — no surviving target re-declared in its file (adjust regex per language):
sqlite3 "$OND" "select distinct t.name, t.file_path from edges e join nodes t on e.target=t.id where e.$F limit 80;" \
 | while IFS='|' read -r n f; do [ -f "$f" ] || continue; \
     c=$(grep -cE "(const|let|var)[[:space:]]+$n\b" "$f"); [ "${c:-0}" -gt 1 ] && echo "LEAK $n x$c $f"; done
# (c) precision sample — eyeball reader->target pairs across the tree:
sqlite3 -column "$OND" "select s.name,'->',t.name from edges e join nodes s on e.source=s.id join nodes t on e.target=t.id where e.$F order by e.id desc limit 12;"
```

对于每个 FP 嫌疑人，打开文件并确认读者是否真正读取该文件范围
目标。一个文件中的 FP 集群 → 修复（扩展防护）。一次性→记录下来，不要追。

### 4.3 Impact-API delta（标题）+代理 A/B

标题指标——价值参考将盲目影响变成真正的影响：

```bash
for s in SOME_CONST ANOTHER_CONST; do
  printf "%-20s ON %s OFF %s\n" "$s" \
    "$("$CG" impact "$s" --path "$ON"  2>/dev/null | grep -oE '— [0-9]+ affected' | head -1)" \
    "$("$CG" impact "$s" --path "$OFF" 2>/dev/null | grep -oE '— [0-9]+ affected' | head -1)"
done
```
从探测器的“首要目标”列表中选择目标。期望 ON ≫ OFF（例如 1 → 90）。

**代理 A/B**（每种语言可选 — 下面的发现与大小/语言无关，因此
确定性探针+影响增量通常就足够了）。如果你运行它：两个**新鲜开/关
索引**，为每个索引预热一个 `--no-watch` 守护进程，`claude -p` 带有 **`--model sonnet
--effort high`**, ≥2 runs/arm. The pattern in `scripts/agent-eval/ab-new-vs-baseline.sh` 是
模板**但它切换构建+重新索引（无标志），这会擦除特定于标志的
索引 — 不要按原样使用它作为标志 A/B。**（内存：`agent-eval-nested-attach`，
`agent-eval-targets-public-oss-only`。）

**已建立的 A/B 发现（不要重新推导）：** 在 exalidraw 上进行 12 次运行时，双臂都做到了
0 Read / 0 Grep — 代理在一次通话中回答影响问题并伸手去拿
`rustcodegraph_search`/`callers`，*不是* `impact`/`explore`，所以它通常不会查询
值引用边缘根本没有。 ON 永远不会比 OFF 更糟糕。 **所以：value-refs 不会减少代理
读取 — 胜利在于爆炸半径的正确性**（影响 API / RustCodeGraph Pro 的判决引擎）。

---

## 5. 每种语言的清单（实际工作）

### A.“值得跟踪的常量”在哪里？ （先决定）

目标门现在接受 **`file:`、`class:` 和 `module:`** 父级。在做任何事情之前：

- 如果语言将可共享常量放在**文件/模块范围**（TS/JS、Python 模块
consts、Go 包变量、Rust 模块/impl `const`/`static`) → 按原样安装；继续。
- 如果常量存在于**类/模块内部**（Ruby — 完成）→ 现在 `class:`/`module:` 门
覆盖它们，但有两件事可能需要首先修复：（1）提取器实际上必须*提取*
类内部常量作为节点（`variableTypes` 分支的调度跳过
类内部分配——Ruby 需要 `constant`-LHS 分配的例外）； （2）
reader-scan 必须匹配，但是语法表示常量 *reference* （Ruby 使用
`constant` 节点，而不是 `identifier`）。请参阅设计文档中的 Ruby 块。
- **类范围精度**使用**文件范围**目标映射（每个文件每个名称一个目标），而不是
严格的同类匹配——因为词法范围语言（Ruby）让嵌套类读取
封闭类的常量，严格匹配会删除那些有效的读取。唯一真正的FP
与一个文件中的 *sibling* 类中的常量名称相同（rails 上约 1.7% 的 Ruby 目标）；
有效的代码很少会遇到它（裸兄弟类常量是 Ruby 中的 NameError）。
- **Java/C#/Kotlin/Swift 类范围常量已完成。** 门现在接受 `file:`/`class:`/
`module:`/**`struct:`/`enum:`** 父级 - 为 Swift 添加了 `struct:`/`enum:` 扩展，其中
命名空间在 `enum`/`struct` (`enum Constants { static let X }`) 中共享常量。 **教训
下一个类范围语言：** 检查示例 const (`select … substr(id…)`) 的 *父类型* — if
是`struct:`/`enum:`/`interface:`，门没有列出，加宽门（一行）或
尽管节点存在，但该功能不会发出任何信息。
- **确认阅读器扫描与语言的常量*参考*节点类型匹配（PHP 课程）。**
`flushValueRefs` 中的阅读器扫描与 `identifier` / `constant` / `name` 匹配。如果新语言
表示一个常量*读取*作为其他节点类型，扫描没有发现任何内容并且**没有边缘形成**
即使目标已正确注册。 PHP 将 const 引用为 **`name`** 节点（裸 `X`，并且
const half `self::X` / `Foo::X`)，在添加 `name` 之前扫描会错过。转储样本
读取器主体（`scripts/agent-eval` 或快速 `getParser` 步行）并检查节点类型
扫描*之前*恒定参考 - 零边缘扫描通常意味着这一点，而不是目标门错误。

### B.确认声明符节点类型（针对shadow prune）

影子剪枝（以 `flushValueRefs` 形式）通过 `switch (n.type)` 对声明符名称进行计数
声明符节点类型 — 一个文件只有自己的语法节点，因此可以安全地列出所有节点
一次切换语言类型。 **在那里添加新语法的声明符类型**，其中
提取绑定名称的正确方法。 **根据实际语法进行验证**（不要相信这个
表——通过解析样本进行确认）。 **这一步是承重的：**如果跳过这一步，修剪
默默地对新语言不做任何事情，并且文件内阴影会产生误报
（这正是第一次 Go 传递时发生的情况 - 请参阅下面的 §5-Go）。

| Language | Declarator nodes | Name extraction | Status |
|---|---|---|---|
| TS/JS/tsx | `variable_declarator` | `namedChild(0)` | 完毕 |
| Go | `const_spec`、`var_spec`、`short_var_declaration` | 规格 → `namedChild(0)`； Short-var → `left` 字段中的标识符 | **完毕** |
| Python | `assignment` | `left`字段：标识符，或者迭代一个`pattern_list`/`tuple_pattern` | **完毕** |
| Rust | `const_item`、`static_item`、`let_declaration` | const/static → `name` 字段；让 → `pattern` 字段 | **完毕** |
| Ruby | `assignment`（LHS是`constant`节点） | 已经在开关中； Ruby 无法本地隐藏常量，因此修剪实际上对它来说是无操作的 | **完成**（类范围） |
| Ruby | `assignment` 具有恒定的 LHS (`CONST`) | 左心室 | 验证 |
| C | 文件范围 `declaration` 中的 `init_declarator` | `cDeclaratorIdentifier` 走 `declarator` 链（init → 指针/数组 → 标识符） | **完毕** |
| C++ | **尝试和恢复** — 解析保真度（请参阅§2b 中的 C++ 注释） | — | 恢复了 |
| Java | `variable_declarator`（字段 AND 方法-本地） | `namedChild(0)` = 名称标识符 — **已经是 TS/JS 情况**，无需新接线 | **完毕** |
| C# | `variable_declarator`（字段 AND 方法-本地） | 与 Java 相同 — 已在 switch 中 | **完毕** |
| PHP | **没有任何** | `$var` 本地 (`variable_name`) 是与裸常量不同的命名空间 - 本地永远不能隐藏常量，因此修剪是无操作并且不需要 PHP 声明符 | **完成**（不适用） |
| Scala | `val_definition`、`var_definition` | `pattern` 字段（标识符）— 捕获由方法本地 `val` 遮蔽的对象/顶级 val | **完毕** |
| Kotlin | `property_declaration` | `variable_declaration → simple_identifier`（并且 `bump` 接受 `simple_identifier`）— 捕获由方法局部 `val` 遮蔽的对象/伴生常量 | **完毕** |
| Swift | `property_declaration` | `<name> pattern → simple_identifier` (`firstSimpleIdentifier`) — prune case 解析 Kotlin 和 Swift 形状；捕获由局部方法 `let` 遮蔽的静态常量 | **完毕** |
| Dart | `static_final_declaration`（目标）+ `initialized_identifier`（现场/`var`）+ `initialized_variable_definition`（本地） | 每个都有一个直接的 `identifier` 子级 — 捕获由方法本地 `const` 遮蔽的顶级/静态常量 | **完毕** |
| Pascal | `declConst`（单位/类常量 = 目标）+ `declVar`（本地 `var`） | `<name>` 字段 — 捕获由局部函数 `const X` 遮蔽的单元 `const X` | **完毕** |

**剪枝规则是`declarators > file-scope-node-count`，而不是`> 1`。**可以绑定名称
合法地*在文件范围*两次 - **条件模块定义**（`try: X = a; except: X = b`，
或 `if cond: X = a else: X = b`）。这些构成 N 个文件范围节点和 N 个声明符，所以它们是
保留；真正的本地影子使声明符超出文件范围节点。 Python强制这样做
细化（try/ except const def 无处不在）；这对所有人来说都更正确
语言。 `fileScopeValueCounts`（在 `captureValueRefScope` 中递增）跟踪文件范围
每个名称的节点数。另外：同名值引用边缘被抑制（`refName !== scope.name`），
因为条件定义的两半否则会交叉引用。

**Go 是“步骤 B 很重要”的有效示例：**第一遍将 `go` 添加到
仅`VALUE_REF_LANGS`，合成探针立即显示出误报——
`func withShadow() { TimeoutSeconds := 5; return TimeoutSeconds }` 获得了优势
`const TimeoutSeconds`，因为修剪扫描了 `variable_declarator`（Go 没有
有）。修复：添加 Go 的 `const_spec`/`var_spec`/`short_var_declaration` 到 switch。请注意
**精度优先权衡** 这是从 TS/JS 继承的 — 为
*整个文件*，因此该文件其他地方的合法阅读器也会失去其优势。随时随地扫一扫
（杜松子酒/雨果/普罗米修斯）这种过度修剪可以忽略不计（保护不变干净，没有泄漏），所以
不值得对每个读者进行分析——但要根据每种语言重新检查。

### C. 确认提取器分配的类型

`captureValueRefScope` 关闭目标的 `kind ∈ {constant, variable}`。索引示例文件
并检查 `select kind,name from nodes where file_path like '%sample%'` — 确认模块级别
常量显示为 `constant`/`variable`（不是 `field`、`property`、`import` 等）。如果他们
出来是别的东西，调整目标门。

### D. 线+扫

1. 将语言字符串添加到 `VALUE_REF_LANGS`。
2. `npm run build`。
3. 在**小型/中型/大型**公共 OSS 存储库（≥3 个尺寸）上运行 §4.1 探测。更喜欢回购协议
具有真正的配置/常量/查找表模块（该功能的亮点）。
4. 对每个运行 §4.2 FP 搜索。修复 FP 簇（扩展防护）；记录单例。
5. 对一些目标运行§4.3影响增量。
6. 将 **矩阵行** 添加到 `value-reference-edges.md`（每种语言），并将 **测试** 添加到
`__tests__/value-reference-edges.test.ts`（正面读取+影子/负面案例）。
7. `npx vitest run __tests__/value-reference-edges.test.ts` 和全套。

**通行证：** 每个尺寸的节点数在开/关时都相同；精密样品干净（FP簇
固定的）;影响增量显示盲目→真实半径获胜；完整测试套件绿色。

---

## 6. Git / PR 工作流程（之前的工作流程是如何完成的）

- 分支 `main`（例如 `feat/value-refs-<lang>`）。这项验证工作一直存在
`feat/value-refs-validation`;一种新的语言可以扩展它或建立自己的分支。
- 纯粹的验证更改是**文档（+测试）**；精确修复是集中的**代码** PR
（如#895）。在可行的情况下，将代码修复与文档/矩阵更新分开。
- 提交消息预告片：`Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`。
- PR车身拖车：`🤖 Generated with [克劳德·科德](https://claude.com/claude-code)`。
- 合并是**维护者的决定**——除非被告知，否则不要自行合并。分支保护需求
授权时为 `gh pr merge --squash --admin`（内存：`gh-merge-needs-admin`）。
- CHANGELOG：`## [Unreleased]` 下面向用户的条目；不要预先创建版本块。

---

## 7. 陷阱已经命中（节省时间）

- **探测错误匹配：** `metadata LIKE '%valueRef%'` 匹配其他边缘中的*文件名*
元数据（例如 `interface-impl` `calls` 边，其 `registeredAt` 为
`…/textModelValueReference.ts`）。 **始终** 过滤 `kind='references' 和元数据，如
'%"valueRef":true%'`。这在 vscode 上创建了一个纯粹的虚拟“方法目标”FP
查询噪声。
- **`searchNodes` 返回 `SearchResult[]`** （`.node` 包装 `Node`） - 在测试中使用
`.map(r => r.node)`。 `getImpactRadius().nodes` 是一个 **`Map`** — 迭代 `.values()`。
- **`RustCodeGraph.initSync(dir, opts)` 忽略 `opts`** — 它只需要路径；默认值
配置索引 `.ts`/`.tsx`/`.js`。不要依赖通过的 `include`。
- **节点计数在开/关时必须相同。**如果不是，则 value-refs 正在（错误地）创建节点
——先调查再说。
- **大型存储库：**索引 vscode（11.5k 文件）每臂占用约 2m 和约 1GB 的数据库；清理
之后为 `/tmp`（每个开/关对为数百 MB 到 >2GB）。
- **require-bindings (CommonJS) 不是 FP** — 请参阅第 3 节。不要“修复”它们。
- **不要为不明显的间隙过度设计防护装置**（例如仅参数阴影）：
仅以证据为依据。维护者转向最小的、外科手术修复。
- **C 宏前缀原型错误解析（C FP 簇）：** 未知的前导宏
（`CURL_EXTERN`，`XXH_PUBLIC_API`）使tree-sitter-c错误解析原型“MACRO RetType”
fn(args);` 作为*声明*，其声明的“变量”是裸返回类型标识符
(`XXH_errorcode`)，将 `fn(args)` 拆分为虚假表达式。它铸造了一种名为
每个原型都是全局的——然后由该类型的每个函数边缘（redis `XXH_errorcode` 1→18）。
这些错误*总是*产生一个**裸`identifier`**声明符（跨过检查
指针/数组/大小返回变体）；真正的常量/表总是有一个 `init_declarator` 和真正的
指针/数组全局声明它们自己。修复 = **跳过 C 中的裸 `identifier` 声明符**
分支。与早期传递相比，“额外”文件范围变量节点也会减少节点数量——双臂
匹配，但不要惊讶修复后计数*较低*。
- **"Easy path" ≠ "nodes already exist."** The §2b table classifies by *scope*;它不承诺
提取语言的常量。 C 位于 easy 列中，但发出了零文件范围 const
节点。对样品运行 §5 步骤 C (`select kind,name from nodes where file_path like '%sample%'`)
*首先* - 如果 const 不存在，那么您正在进行 Ruby 处理，而不是简单的路径。
- **类常量可以提取为 `field` 类型，而不是 `constant` (Java/C#)。** 步骤 C 必须检查
*善良*，不仅仅是一个节点的存在：Java `static final` 和 C# `const`/`static readonly` 出现了
作为 `field`，值参考目标门（仅限 `constant`/`variable`）默默地拒绝 - 所以
尽管节点存在，但该功能什么也没发出。 Fix = 上的 `isConst` 谓词
提取器（在 const 修饰符上门控）+ `extractField` 中的一种开关（按语言划分范围，因此
其他语言的字段保留为 `field`)。不要扩大*门*来接受`field`——这会拉扯
在每个可变实例字段中作为目标。并且只有 const *subset* 转换：Java 实例
`final` 或 C# 实例 `readonly` 是每个对象状态，必须保持 `field`。
- **具有正确注册目标的零边缘扫描 = 读取器扫描节点类型（PHP 陷阱）。**
目标可以完美注册（正确的种类，正确的范围）并且*仍然*产生零边缘，如果
reader-scan 无法识别该语言如何写入常量 *read*。 PHP 将 const 引用为
**`name`** 节点，而不是 `identifier`/`constant`，因此扫描什么也没看到，直到 `name` 添加到
匹配。在假设稀疏/空扫描上存在目标门错误之前，转储读取器主体并检查
已知常量引用的节点类型。 （将引用节点类型添加到扫描中是安全的
语言 - `flushValueRefs` 仅针对值引用集运行，并且文件仅保存其自己的
语法的节点； `name` 在当前集合中仅支持 PHP。）
- **仅相同文件意味着跨文件重的语言产量较少 - 这是正确的，不是错过。** PHP
跨文件读取常量远远多于一个文件内的常量（到处都是 `Logger::DEBUG`），所以 laravel
（2,956 个文件）仅提供 86 个边，而 Ruby Rails 提供 2,255 个边。不要追逐它：跨文件价值消费者
超出*每种*语言的范围（需要导入/范围解析）。报告较低产量
诚实地在矩阵中，而不是将其视为需要修复的错误。
- **某些提取器在错误范围内将参数/字段发出为 `variable` — 限制为 `constant`
（Pascal 陷阱）。** Pascal 的提取器将函数 `const`/`var` 参数和类字段发出为
`variable` 是封闭单元/类的父级，因此它们通过目标门并崩溃到嘈杂
文件范围目标（`Dest`、`aItem` 读作“无处不在”）。真正的共同价值观都是`constant`
(`declConst`)，因此修复是 `captureValueRefScope` 中的单行每种语言限制：Pascal
仅针对 `constant`。在信任新语言的 `variable` 目标之前，先对它们进行采样 - 如果它们是
参数或实例字段而不是模块/全局状态，限制为 `constant`。 （残差
tail 仍然可能泄漏：tree-sitter-pascal 上下文相关地错误解析了复杂中的 `const` 参数
Delphi 签名为 `declConst` — 一个小的解析保真 FP，被接受为记录的警告。）
- **存在目标的零边缘扫描可以是阅读器侧，而不仅仅是阅读器扫描节点类型
（Dart 陷阱）。** 目标提取良好，读取器范围已注册，读取器扫描节点类型正确 —
并且边缘仍然为零，因为 Dart 附加了一个方法 **body 作为签名的下一个 *sibling***
节点（它被存储为读取器范围），因此扫描仅遍历签名子树。
如果语言的函数/方法体不是您注册为阅读器范围的节点的后代，
扫描不会看到读数——拉入同级/链接体。当边缘为零时检查此项，但
目标和读取器节点看起来都是正确的。

---

## 8. 参考资料

- 代码：`src/extraction/tree-sitter.ts`（`VALUE_REF_LANGS`、`captureValueRefScope`、
`flushValueRefs`)、`src/extraction/generated-detection.ts` (`isGeneratedFile`)。
- 设计+矩阵：`docs/design/value-reference-edges.md`。
- 测试：`__tests__/value-reference-edges.test.ts`。
- PR：**#895**（默认开启 + 阴影修剪），**#897**（TS/JS/tsx 验证）。
- 记忆：`value-reference-edges-default-on`、`agent-eval-targets-public-oss-only`、
`agent-eval-nested-attach`、`gh-merge-needs-admin`、`impact-coverage-findings`。
