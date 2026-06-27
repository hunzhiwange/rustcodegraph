# 函数即值捕获 (#756) — 回调的注册链接

**问题。** 用作*值*的函数 - 作为参数传递，分配给
函数指针或字段，放置在结构初始值设定项或处理程序表中 -
在 19 种树木保护者语言中的任何一种中**没有优势**（2026 年 6 月 11 日调查；
0/19）。 C 回调上的 `callers(my_recv_cb)` 仅显示直接调用，因此
每个注册的回调看起来都死了，注册站点——代理的
实际的下一个问题（“这是在哪里连接的？”）——是看不见的。

**非目标，故意的。** 解决*调度*（`o->cb(x)`→具体
注册函数）需要通过结构字段的数据流；甚至 LSP 也需要
那里有后备（参见 #756 线程）。部分覆盖比没有覆盖更糟糕
错误的立场比沉默更糟糕——派遣决议仍未被发现。什么
ships 是*注册*端，它是确定性的：函数的名称
实际上是在注册站点的源代码中。

## 机制

```
capture (tree-sitter.ts walkers, table-driven per language: src/extraction/function-ref.ts)
   → gate (flushFnRefCandidates: same-file fn/method name ∪ imported binding names;
            C-family file-scope initializers skip the gate — see below)
   → unresolved ref, referenceKind 'function_ref' (internal-only kind)
   → resolution (resolveOne branch: resolveViaImport first, then matchFunctionRef —
                 exact name, function/method kinds only, same-family, same-file first,
                 cross-file only when UNIQUE, never fuzzy)
   → edge kind 'references', metadata { fnRef: true, resolvedBy, confidence }
```

`getCallers`/`getCallees`/`getImpactRadius` 已经遍历了 `references`，所以
注册站点表面没有图形层变化。 MCP 调用者/被调用者
列表将它们标记为“通过回调注册”。

捕获来自三个步行者的火焰（一个节点只被一个步行者访问过）：
`visitNode`（文件/类范围）、`visitForCallsAndStructure`（函数体）、
`visitPascalBlock`（帕斯卡体）。步行者消耗的子树没有
降序（顶级变量初始值设定项、类字段/属性初始值设定项、
自定义 `visitNode` 钩子（如 Scala 的 val/var 处理程序）获得仅限候选人
`scanFnRefSubtree` 在嵌套函数边界处停止。

## 每种语言的价值位置（经过探测验证）

| 语言 | 精氨酸 | 指定右侧 | 键控初始化 | 列表/表格 | 包装形式 |
|---|---|---|---|---|---|
| C/对象C | `argument_list` | `assignment_expression.right` | `initializer_pair.value` | `initializer_list`、`init_declarator.value` | `&fn` (`pointer_expression`)、`@selector(...)` (ObjC) |
| C++ | **`&` 仅在 args/rhs/varinit 中形成** | （相同 — 仅显式 `&`） | 仅限 FILE 范围内的裸 ID | 仅限 FILE 范围内的裸 ID | `&fn`、`&Cls::method`（解析范围为类） |
| TS / JS (tsx/jsx) | `arguments` | `assignment_expression.right` | `pair.value` | `array`、`variable_declarator.value` | `this.method`（`member_expression`，类范围 - 参见规则 3） |
| Python | `argument_list`、`keyword_argument.value` | `assignment.right` | `pair.value` | `list` | `self.method` (`attribute`) |
| 去 | `argument_list` | `assignment_statement` / `short_var_declaration` (`expression_list`) | `keyed_element` | `literal_value`、`var_spec.value` | — |
| 锈 | `arguments` | `assignment_expression.right` | `field_initializer.value` | `array_expression`、`static_item` / `let_declaration.value` | — |
| 爪哇 | `argument_list` | `assignment_expression.right` | — | `variable_declarator.value` | `method_reference` (`Cls::m`, `this::m`) — 唯一形式 |
| 科特林 | `value_arguments` | `assignment`（最后一个孩子） | — | — | `callable_reference` (`::f`), `navigation_expression` `this::m` |
| C# | `argument_list` (`argument`) | `assignment_expression.right`（包括`+=`） | — | `initializer_expression`、`variable_declarator` | `this.M`（`member_access_expression`；供应语法使 `this` 保持匿名 - 已处理） |
| 红宝石 | `argument_list` | — | `pair.value` | — | 仅 `method(:sym)` / `&method(:sym)` — 裸 ID 是 Ruby 中的调用/本地变量 |
| 迅速 | `value_arguments` (`value_argument.value`) | `assignment.result` | （标记为 ctor args = args） | `array_literal`、`property_declaration.value` | `#selector(...)` |
| 斯卡拉 | `arguments` | `assignment_expression.right` | — | `val_definition.value`（通过钩子扫描） | 预计时间 `fn _` (`postfix_expression`) |
| 镖 | `arguments` (`argument`) | `assignment_expression.right` | `pair.value` | `list_literal`、`static_final_declaration` | — |
| 卢阿/卢奥 | `arguments` | `assignment_statement` (`expression_list.value`) | `field.value`（键控+定位） | （相同的） | — |
| 帕斯卡 | `exprArgs`（通过 `visitPascalBlock`） | `assignment.rhs` (`OnFire := Handler`) | — | — | `@Handler` (`exprUnary.operand`) |
| PHP | 字符串可调用项仅作为已知核心 HOF 的参数（`usort`、`array_map`、`call_user_func*`… — `PHP_CALLABLE_HOFS`），非门控 + 唯一或删除（不导入 PHP 全局变量） | — | — | — | `[$this, 'm']` → 类范围 `this.m`； `[Foo::class, 'm']`→合格； `'Cls::m'`→合格；一流的可调用 `fn(...)` 已提取为 `calls` |
| 红宝石钩 | `（跳过_）？（之前\|后\|around)_*` + `validate`/`set_callback`/`helper_method`/`rescue_from(with:)` symbols → class-scoped `this.<sym>` (rides the supertype pass: `before_action :authenticate` → ApplicationController). `validates` (复数) 排除 - 其符号是属性 | — | — | — | 任何其他调用下的符号都不会产生任何结果 |

## 精确规则（每一项均由真实回购误报购买）

1. **门**（提取时间）：只有名字匹配的候选者才能生存
同一文件的函数/方法或 **导入的绑定** (`referenceKind ===
'imports'` only — scraping type-annotation `references` 名称让当地人知道
通过共享类型成员的名称； Excalidraw）。
2. **C 系列非门控文件范围**：C 没有符号导入和寄存器
以 repo 规模跨文件回调（redis `server.c` 的命令表名称
来自 `t_*.c` 的处理程序）。文件范围初始值设定项位置 (`value`/`list`
模式）跳过门——安全，因为 C 文件范围初始值设定项是
**常量表达式上下文**：一个裸标识符，只能有一个
函数地址（枚举/宏名称被种类过滤器删除）。当地的
初始化器和赋值保持门控：`prev = next`、`*str = field`、
`arena_ind_prev = arena_ind` (redis/jemalloc) 每个都匹配一个唯一的
当 `rhs`/`varinit` 时，某处有同名函数并产生错误的边缘
没有门控。
3. **TS/JS/Python：裸 id 仅解析为 `function` 类型。** 裸
在这些语言中，标识符永远不能是方法值（方法需要一个
接收器 - `this.m` / `self.m`），因此允许方法目标吸收
作为参数传递的局部变量 (`new Set(selectedPointsIndices)`;
docopt.py 的 `name`/`match` 参数 — excalidraw/fmt A/B 结果）。
TS/JS `this.X` 值被捕获为 `this.`-PREFIXED 候选值，并且
已解决 CLASS-SCOPED（`resolveThisMemberFnRef` 中
`src/resolution/index.ts`)：目标必须是一个函数/方法，其
限定名称共享来自符号的类前缀，同一文件，无
任何类型的后备 — `addEventListener(…, this.onResize)` 命中
封闭类的方法； `this.fonts`（属性，#808 后的字段
分类）和继承/未知成员没有优势。蟒蛇的
`self.m` 形式通过其自己的捕获形状保留方法目标。
C#/Swift/Dart/Java/Kotlin 保留方法目标（方法组、
隐式自我，方法引用是真正的方法值）。
4. **C++ 是 `&` 显式** (`addressOfOnly`)：裸标识符仅符合
文件范围初始化表；其他地方（参数、作业、本地
braced-init 列出 `{begin, size}`) 仅 `&fn` / `&Cls::method` 计数。
C++ 代码库中充满了通用的自由函数名称（`begin`、`end`、
`out`、`size`、`data`) 与当地人发生碰撞，并且是 OUT-OF-LINE 成员
定义提取为 *function*-kind 节点，击败 kind 过滤器 -
fmt 上的裸 ID 匹配大部分是错误的边缘（72 通用名称 + 105
成员/宏不匹配 → 规则之后：22 条边，~20 条真正的 gtest
成员指针接线）。 `&x` vs `*x` 共享 C 的 `pointer_expression`；仅有的
`&` 操作员符合资格。 `&Cls::method` 将 SCOPED 解析为该类。
5. **Swift 重载族拒绝**：一个文件中的多个同名方法
(`Session.request(...)` × N) + 一个裸标识符 = 几乎总是一个
同名参数，而不是方法值 (Alamofire) — 拒绝而不是
猜测。一个独特的方法（SwiftUI `action: handleTap`）仍然可以解决。
6. **参数向前跳过**：`this.status = status` / `o->cb = cb`（分配
其成员名称等于 RHS 标识符）和 Swift/Kotlin 标记的 args
`value: value` — 转发的本地/参数，其函数值为
不可知的；其他地方的同名函数将是错误的目标。
7. **解构跳过**：`const { center } = ellipse` 提取数据，从不
函数别名。
8. **生成/缩小的文件**（`*.min.js` 和代码生成模式
`generated-detection.ts`) 不产生 fn-ref 候选者 — 缩小
单字母符号在任何地方都可以解析（Alamofire 的供应商 jquery）。
9. **解决方案**：仅函数/方法类型，相同的语言系列，从不
ref 自己的节点（无自循环），同文件首先匹配，仅当出现跨文件时
这个名字是独一无二的——歧义导致**没有优势**。没有模糊后备，
曾经（`matchReference` 短路 `function_ref` 参考
`matchFunctionRef`）。
10. **失控不变量** (#760)：`matchFunctionRef` 总是返回
`original: ref` — 存储的行 — 所以 `deleteSpecificResolvedReferences`
排出批次。

## 验证（2026-06-11，EXTRACTION_VERSION 19）

无隐藏 A/B（基线 = `main` 的工作树），新鲜的浅克隆，公共
仅限操作系统。每个存储库：节点数必须相同，`calls` 边必须相同，
`references` 严格加法，精密抽查源码
采样 `fnRef` 边缘的线。

最终构建，所有 17 个存储库（节点相同，并且在每个存储库上调用边缘未受影响）
排; `unresolved_refs` 完全耗尽 — 无批量旋转变压器失控）：

| 郎 | 回购协议 | 节点（基础=固定） | 调用 Δ | 获得的裁判数 | 笔记 |
|---|---|---|---|---|---|
| C | 雷迪斯 | 18931 | 0/0 | **+1918** | 30/30 正版示例 — ops 表、qsort 比较器、模块注册、lua lib 表 |
| TS/反应 | 外画 | 10299 | 0/0 | **+121** | 18/20 —残差 = 参数遮蔽导入的函数（文件级 dep real） |
| 去 | 杜松子酒 | 2599 | 0/0 | +14 | |
| 锈 | 字节 | 第947章 | 0/0 | +76 | `map(fn)`，结构初始化 |
| 爪哇 | 好的http | 16008 | 0/0 | +2 | 根据设计，仅 method-ref 形式 |
| 科特林 | 奥基奥 | 7801 | 0/0 | +1 | 仅 `::fn` 形式，设计使然 |
| 迅速 | 阿拉莫费尔 | 3477 | 0/0 | +116 | 对抗性案例（参数镜像 API 名称）；重载族 + label== 应用名称规则 |
| Python | 烧瓶 | 2705 | 0/0 | +111 | 8/8 正品样品 — 包括。 `ensure_sync(self.dispatch_request)` |
| 红宝石 | 西纳特拉 | 第1751章 | 0/0 | +8 | 仅 `method(:sym)` |
| C# | 牛顿软件 | 20208 | 0/0 | +38 | 方法组，`+=` |
| 斯卡拉 | 范围 | 第694章 | 0/0 | +10 | eta 扩展 |
| 镖 | 提供者 | 第1154章 | 0/0 | +73 | 隐式-this getter 读取 — 真正的同类依赖关系 |
| 卢阿 | 破获 | 第1257章 | 0/0 | +14 | |
| 卢奥宴会 | 融合 | 2126 | 0/0 | +18 | `:Connect(fn)` |
| 对象C | AF网络 | 第1487章 | 0/0 | +52 | `@selector`，目标-行动 |
| 帕斯卡 | 帕斯卡币 | 48788 | 0/0 | +577 | `OnClick :=` 事件接线 + 无括号调用引用（参见限制） |
| C++ | FMMT | 7345 | 0/0 | +22 | ~20/22 真正的 gtest 成员指针管道在 addressOfOnly 之后 |

Redis 上的索引成本：+6% 时间，+5% 数据库大小。

## 已知限制（记录的、有意的）

- **调度解析**（`o->cb(x)`→实现）：未覆盖，见上文。
- **C 交叉文件位于门控位置**：通过注册的外部回调
*赋值*位于与其定义不同的文件中，仅当以下情况时才会解析
名称是 repo-unique （初始化表没有这个限制 - 它们是
在文件范围内未门控）。
- **C++ 裸名注册**（`register_handler(my_cb)` 不含 `&`）：
下降了 `addressOfOnly` — 通用名称冲突率导致 ID 裸露
真实 C++ (fmt) 上的净负值。 `&my_cb` / 文件范围表涵盖
习语； C 文件保留裸参数。
- **本地/参数隐藏导入或同文件函数**
（`mutateElement(newElement, …)`，其中文件还导入 `newElement`；
JS 插件的 `indexOf(val)` 与同文件 `val()` 助手）：不可约
没有本地范围的跟踪——数据流边界故意离开
裸露。在回调密集的存储库中，每 20 个采样边大约 1-2 个；文件级
在每一个观察到的案例中，依赖性都是真实存在的。
- **Swift 同类参数冲突** (`eventMonitor?.request(self,
didFailTask​​：任务...）` where the enclosing type ALSO has a `task`方法）：
封闭类型作用域（隐式 self 方法仅匹配 from 符号的
自己的类型，顶级裸 ID 永远不会匹配方法）消除了跨类
Alamofire 上的碰撞类（−44 个错误边），但参数命名为
SAME 类型的方法在静态上与
隐式自我方法值。残留，有记录。
- **Pascal 无参数调用** (`Result := DoInitialize`)：捕获为
引用（Pascal 无法区分过程 VALUE 和无括号
不带类型的 CALL）。依赖方向是正确的，这些调用
以前是完全看不见的（#791）——严格来说更真实，不完美
标签。
- **Java/Kotlin 方法通过变量引用** (`subscriber::onNext`,
`m::run0`)：接收器类型静态未知 - 故意没有边缘（
obj.方法类）。 RxJava 的基线裸捕获将这些解析为
同名同文件方法（“注册”匿名的测试方法
类的 `onNext`);合格的返工将其丢弃。 `Type::method` 解决
跨文件（范围基于相同文件类型∪导入的名称，包括最后一个
虚线 JVM 导入段）； `this::m` / `super::m` 乘坐
类范围+超类型路径。
- **合格的 `Type::member` 候选人跳过姓名门**（如 `this.X`）：
Java/Kotlin 同包引用和 Kotlin 同伴不需要导入，
所以门永远无法看到它们的范围——而显式引用语法是
自我选择，同时分辨率保持范围后缀锚定 +
unique-or-drop（`Decoy::handle` 不能匹配 `KtHandlers::handle` 引用）。
这也是解析同伴成员参考的原因：companies extract
真实透明（`KtHandlers::handle`，类的方法）
多行代码。 （单线 `class X { companion object { … } }` 是
上游 tree-sitter-kotlin 错误解析 — 错误节点 — 并且只出现过
在我们自己的探针夹具中；别追它。）
- **Swift 跨文件裸引用**：Swift 可以看到模块范围的符号，而无需
导入，因此跨文件裸回调仅在 repo-unique 时解析
（函数；方法仅是封闭类型）。十字型`#selector`
目标（罕见——目标行动通常是自我）也被排除在外。
- **`obj.method` 成员值**，其中 `obj` 不是 `this`/`self`：延迟 —
如果没有本地数据流，接收器的类型静态地是不可知的。
- **已知 HOF 位置之外的 PHP 字符串**（裸 `'handler'` 到
任意函数；框架注册表，例如 WordPress `add_action`）：
故意不捕获——字符串只有在作为可调用对象时才值得信赖
已知的可调用位置。框架注册表属于 `frameworks/`
解析器（如果已添加）。 **钩子 DSL 之外的 Ruby 符号同样如此。
- **超类型传递是 NODE 锚定的**（文件锚定类节点→
实现/扩展边缘目标 → `contains` 锚定成员查找）：a
name-keyed `getSupertypes('Engine')` 联合了每个 Rails `Engine` 的父母
并产生了跨阶级的错边；节点行走消除了它
（导轨+440→+385，所有采样边缘都是正品）。
- **`this.X` 继承成员通过超类型传递解析**
（`resolveDeferredThisMemberRefs`，实现/扩展上的深度上限 BFS，
在边缘持续后运行 - 与 #750 一致性传递相同的生命周期）。
将 getter 读入本地 (`const s = this.snapshot`) 仍然会产生
引用边缘到吸气剂——一个不完美的真正依赖
“登记”的味道。
