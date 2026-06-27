# RustCodeGraph 语言验证指南

您正在验证 RustCodeGraph 是否完全支持特定的编程语言。用户将为您提供一条通往本地克隆的真实、流行的开源代码库的路径。您的工作是使用 RustCodeGraph 的 API 运行一系列真实的提示，并验证结果是否足以表明该语言被**覆盖和支持**。

直到 LLM 能够可靠地使用 RustCodeGraph 的 MCP 工具来导航该代码库（找到正确的符号、理解调用链、探索子系统以及获取实际任务的有用上下文）时，语言才会得到验证。

## 设置

### 1. 构建和索引

```bash
npm run build
rm -rf <codebase_path>/.rustcodegraph
target/release/rustcodegraph init -iv <codebase_path>
```

`-iv` 标志提供详细输出，显示提取进度、节点/边计数和计时。

### 2. 快速健全性检查

```bash
# Verify nodes were extracted with proper qualified names
sqlite3 <codebase_path>/.rustcodegraph/rustcodegraph.db \
  "SELECT name, kind, qualified_name FROM nodes WHERE kind = 'method' LIMIT 10;"

# GOOD: file.go::StructName::method_name  (owner type present)
# BAD:  file.go::file.go::method_name     (owner type missing — needs get_receiver_type)

# Check edge counts
sqlite3 <codebase_path>/.rustcodegraph/rustcodegraph.db \
  "SELECT kind, COUNT(*) FROM edges GROUP BY kind ORDER BY COUNT(*) DESC;"

# Check node kind distribution
sqlite3 <codebase_path>/.rustcodegraph/rustcodegraph.db \
  "SELECT kind, COUNT(*) FROM nodes GROUP BY kind ORDER BY COUNT(*) DESC;"
```

如果方法在 `qualified_name` 中缺少其所有者类型，请先修复该问题（请参阅 [添加 get_receiver_type](#添加-get_receiver_type)），然后再继续进行完整的测试电池。

## 测试电池

针对代码库运行以下**所有**测试类别。直接使用 Rust CLI 或确定性的 MCP 探针；旧的 TypeScript/JS dist 运行时已经退役。调整查询以匹配您正在测试的代码库中的实际类型、方法和子系统。

**每项测试的通过标准：** 结果是否为法学硕士提供了足够正确的信息来回答问题或完成任务？如果您是法学硕士，您会相信这些结果吗？

---

### 测试 1：`rustcodegraph_explore` — 深度探索（最重要）

这是法学硕士使用的主要工具。它必须返回按文件分组的相关源代码，并具有正确的关系，以进行自然语言查询。使用**至少 5 种不同的查询类型**进行测试：

```bash
target/release/rustcodegraph explore --path <codebase_path> "How does the caching system work?"
target/release/rustcodegraph explore --path <codebase_path> "CacheBuilder configuration and build process"
target/release/rustcodegraph explore --path <codebase_path> "How are errors handled and propagated?"
target/release/rustcodegraph explore --path <codebase_path> "How does data flow from input to storage?"
target/release/rustcodegraph explore --path <codebase_path> "How does eviction decide which entries to remove?"

# Deterministic MCP-probe equivalent, useful for regression notes:
target/release/rustcodegraph agent-eval probe-explore <codebase_path> "CacheBuilder configuration and build process"
```

**每个查询要检查什么：**
- 问题的切入点有意义吗？
- 是否出现了正确的文件（不仅仅是测试文件或不相关的代码）？
- 是否存在边缘类型的混合（调用、包含、扩展、实现）——而不仅仅是 `contains`？
- 节点数感觉合适吗？太少（<5）意味着搜索失败。太多不相关的内容意味着噪音。

---

### 测试 2：`rustcodegraph_search` — 符号查找

测试搜索特定符号是否返回排名正确的正确结果。

```bash
target/release/rustcodegraph query --path <codebase_path> --kind class "CacheBuilder"
target/release/rustcodegraph query --path <codebase_path> --kind method "CacheBuilder build"
target/release/rustcodegraph query --path <codebase_path> --kind method "get"
target/release/rustcodegraph query --path <codebase_path> --kind interface "Cache"
target/release/rustcodegraph query --path <codebase_path> --kind enum "Strength"

# Add -j for machine-readable output when you need exact ranking assertions.
target/release/rustcodegraph query --path <codebase_path> --kind method -j "CacheBuilder build"
```

**检查内容：**
- 目标符号是否排在前 3 名？
- 对于 `get` 等常见名称，结果是否包含有助于消除歧义的限定名称？
- 是否存在零结果查询？这是一个错误。

---

### 测试 3：`rustcodegraph_callers` / `rustcodegraph_callees` — 调用链跟踪

测试调用关系是否已正确提取。

```bash
target/release/rustcodegraph callees --path <codebase_path> --limit 20 "build"
target/release/rustcodegraph callers --path <codebase_path> --limit 20 "build"
target/release/rustcodegraph callees --path <codebase_path> --limit 20 "get"
target/release/rustcodegraph callers --path <codebase_path> --limit 20 "get"
target/release/rustcodegraph callees --path <codebase_path> --limit 20 "put"
target/release/rustcodegraph callers --path <codebase_path> --limit 20 "put"
target/release/rustcodegraph callees --path <codebase_path> --limit 20 "invalidate"
target/release/rustcodegraph callers --path <codebase_path> --limit 20 "invalidate"
```

**检查内容：**
- 方法有调用者和被调用者吗？如果方法两者都为 0，则边缘提取可能会被破坏。
- 调用者/被调用者有意义吗？ `build()` 方法应该调用类似构造函数的东西，并由设置/初始化代码调用。
- 计数是否合理？流行代码库中的核心方法应该有多个调用者。

---

### 测试 4：`rustcodegraph_impact` — 变更影响分析

测试影响半径是否正确识别受影响的代码。

```bash
target/release/rustcodegraph query --path <codebase_path> --kind class "<CoreClass>"
target/release/rustcodegraph query --path <codebase_path> --kind interface "<CoreClass>"
target/release/rustcodegraph impact --path <codebase_path> --depth 2 --limit 50 "<CoreClass>"

# Use JSON when you need to group affected symbols by file in a script.
target/release/rustcodegraph impact --path <codebase_path> --depth 2 --limit 50 -j "<CoreClass>"
```

**检查内容：**
- 更改核心接口/类是否会产生广泛的影响半径？
- 受影响的文件是否合理（导入/扩展/使用它的东西）？
- 冲击半径非空吗？对核心类型的零影响意味着边缘缺失。

---

### 测试 5：边缘提取质量

直接验证是否正在为此语言提取主要边缘类型。

```bash
sqlite3 <codebase_path>/.rustcodegraph/rustcodegraph.db "
  SELECT kind, COUNT(*) as cnt FROM edges GROUP BY kind ORDER BY cnt DESC;
"

sqlite3 <codebase_path>/.rustcodegraph/rustcodegraph.db "
  SELECT kind, COUNT(*) as cnt
  FROM edges
  WHERE kind IN ('contains', 'calls', 'imports', 'extends', 'implements')
  GROUP BY kind
  ORDER BY kind;
"
```

**检查内容：**
- `contains` 应该是最常见的（结构层次）。
- `calls` 应该是充足的 - 如果接近零，则该语言的呼叫提取会被破坏。
- `imports` 应该存在——如果为零，则导入解析被破坏。
- 如果语言具有继承性，则 `extends` 和 `implements` 应该存在 - 如果为零，则 `extract_inheritance()` 可能无法处理该语言的 AST。

---

### 测试6：节点提取完整性

验证是否正在提取所有预期的节点类型。

```bash
sqlite3 <codebase_path>/.rustcodegraph/rustcodegraph.db "
  SELECT kind, COUNT(*) as cnt FROM nodes GROUP BY kind ORDER BY cnt DESC;
"
```

**每种语言要检查的内容：**

| 节点种类 | 预期的？ | 笔记 |
|-----------|-----------|-------|
| `file` | 总是 | 每个源文件一个 |
| `class` | 如果语言有阶级 | |
| `method` | 如果语言有方法 | 应包含 `qualified_name` 中的所有者类型 |
| `function` | 如果语言有顶层函数 | |
| `interface` | 如果语言有接口/协议 | |
| `enum` | 如果语言有枚举 | |
| `enum_member` | 如果语言有枚举 | 枚举内的值 |
| `import` | 总是 | 每份进口声明一份 |
| `variable` / `field` | 通常 | 字段、常量、顶级变量 |
| `struct` | 如果语言有结构 | Go、Rust、C、Swift |
| `trait` | 如果语言有特点 | 锈 |

如果预期节点类型的计数为 0，则语言提取器缺少该 AST 类型。

---

### 测试 7：现实世界的 LLM 提示

这是最后也是最重要的测试。模拟开发人员实际会向使用 RustCodeGraph 的法学硕士提出的问题。对于每个提示，运行 `rustcodegraph explore` 或 `rustcodegraph agent-eval probe-explore`，并评估返回的上下文是否能让 LLM 给出正确、完整的答案。

**运行至少 5 种提示样式，适应实际代码库：**

```bash
target/release/rustcodegraph explore --path <codebase_path> "How does the cache eviction policy work?"
target/release/rustcodegraph explore --path <codebase_path> "Where is the LRU eviction logic implemented?"
target/release/rustcodegraph explore --path <codebase_path> "What code triggers cache invalidation?"
target/release/rustcodegraph explore --path <codebase_path> "If I change the Cache interface, what else is affected?"
target/release/rustcodegraph explore --path <codebase_path> "How does CacheBuilder connect to LocalCache?"
target/release/rustcodegraph explore --path <codebase_path> "What happens when a cache entry expires?"
target/release/rustcodegraph explore --path <codebase_path> "What classes implement the Cache interface?"
target/release/rustcodegraph explore --path <codebase_path> "Cache entries are not being evicted when they should be — where should I look?"

# For flow regressions, prefer the probe so results are comparable across runs:
target/release/rustcodegraph agent-eval probe-explore <codebase_path> "What happens when a cache entry expires?"
```

**每个提示要检查什么：**
- 它返回入口点吗？零入口点=彻底失败。
- 切入点与问题**相关**吗？ （不仅仅是碰巧共享一个单词的随机符号。）
- 它跨越多个文件吗？大多数真正的问题涉及跨文件理解。
- 存在关系吗？法学硕士需要了解符号如何连接，而不仅仅是名称列表。
- **你**能够从这种背景下回答这个问题吗？

---

## 诊断故障

| 症状 | 可能的原因 | 在哪里修复 |
|---------|-------------|--------------|
| `qualified_name` 中的方法缺少所有者类型 | 语言需要 `get_receiver_type` | `src/extraction/languages/<lang>.rs` |
| `rustcodegraph_explore` 返回不相关的文件 | 通用名称充斥 FTS；托管提升没有帮助 | `src/db/queries.rs`、`src/context/index.rs` |
| 零 `calls` 边缘 | `callTypes` AST 节点类型缺失或错误 | `src/extraction/languages/<lang>.rs: callTypes` |
| 零 `extends`/`implements` 边缘 | `extract_inheritance()` 不处理该语言的 AST | `src/extraction/tree_sitter/type_refs.rs: extract_inheritance()` |
| 缺少节点类型（没有枚举，没有接口） | 提取器中未列出 AST 类型 | `src/extraction/languages/<lang>.rs: enum_types`、`interface_types` 等 |
| 从查询中删除搜索词 | 术语位于停用词列表中 | `src/search/query_utils.rs: STOP_WORDS` |
| `qualified_name` 缺少嵌套方法的类 | 提取未正确遍历父堆栈 | `src/extraction/tree_sitter/core.rs: visit_node()` |
| 导入边缺失 | `extractImport` 对于此语法返回 null | `src/extraction/languages/<lang>.rs: extractImport` |
| 宏命名空间中缺少 C++ 类/结构/枚举 | 像 `NLOHMANN_JSON_NAMESPACE_BEGIN` 这样的宏会导致树守护者将命名空间块误解析为 `function_definition` | `src/extraction/languages/c_cpp.rs: isMisparsedFunction` 过滤不良名称； `src/extraction/tree_sitter.rs: visitFunctionBody` 提取结构节点 |
| `.h` 标头中缺少 C++ 类 | `.h` 文件默认为 `c` 语言，其中包含 `class_types: []` | `src/extraction/grammars.rs: looks_like_cpp()` — 当检测到 C++ 模式时，基于内容的启发式将 `.h` 文件提升为 `cpp` |
| 模块内的 Ruby 方法在 `qualified_name` 中缺少所有者 | Ruby `module` AST 节点未提取 | `src/extraction/languages/ruby.rs: visit_node` 钩子提取模块； `src/extraction/tree_sitter/core.rs: is_inside_class_like_node` 包括 `module` 类 |
| TypeScript 抽象类缺失 | `abstract_class_declaration` 不在 `class_types` 中 | `src/extraction/languages/typescript.rs: class_types` — 添加 `abstract_class_declaration` |
| 单表达式箭头函数悄然下降 | `extractName` 在表达式主体中查找标识符而不是返回 `<anonymous>` | `src/extraction/tree_sitter.rs: extractName` — 跳过 `arrow_function`/`function_expression` 节点的标识符搜索 |
| Kotlin 接口/枚举提取为类 | `class_declaration` 首先匹配 `class_types`； `interface_types`/`enum_types` 永不起火 | `src/extraction/languages/kotlin.rs: classify_class_node` 检测 AST 子项中的 `interface`/`enum` 关键字 |
| Kotlin 函数提取了零个调用 | Tree-sitter 语法不使用字段名称，因此 `get_child_by_field(node, "function_body")` 返回 None | `src/extraction/languages/kotlin.rs: resolve_body` 按类型查找正文（`function_body`、`class_body`、`enum_class_body`） |
| Kotlin `navigation_expression` 调用未完全解决 | `navigation_expression` 变成了 `get_node_text`，产生带括号的混乱名称 | `src/extraction/tree_sitter/calls.rs: extract_call` — 通过从 `navigation_suffix > simple_identifier` 中提取方法名称来处理 `navigation_expression` |
| Kotlin `fun interface` 声明不可见 | Tree-sitter-kotlin 不支持 `fun interface` 语法 (Kotlin 1.4+)，产生错误或误解析为 `function_declaration` | `src/extraction/languages/kotlin.rs: visit_node` 检测到三种错误解析模式：(1) ERROR 节点 + lambda 主体，(2) 带有 `user_type("interface")` 直接子级的 function_declaration + ERROR 子级中的名称，(3) 带有包含 `user_type("interface")` + 名称的 ERROR 子级的 function_declaration。 `is_fun_interface_node` 检查直接和 ERROR 嵌套的 `user_type` 子项 |
| 当存在嵌套 `fun interface` 时，Kotlin 类/接口方法丢失 | Tree-sitter 将父主体错误解析为 ERROR（以 `{` 开头）+ class_body（嵌套接口主体）； `resolve_body` 发现错误的身体 | `src/extraction/languages/kotlin.rs: resolve_body` 更喜欢以 `{` 开头的 ERROR 主体； `visit_node` 从 `fun interface` 检测中排除类身体错误 |
| Svelte `$props()` 解构产生丑陋的变量名称 | `let { x, y } = $props()` 有 `object_pattern` 作为变量名节点； `get_node_text` 返回完整图案 | `src/extraction/tree_sitter/variables.rs: extract_variable` 跳过 `object_pattern`/`array_pattern` 命名声明符 |
| Svelte 模板函数调用不可见（例如 `class={cn(...)}`） | SvelteExtractor 仅解析 `<script>` 块，缺少模板标记中的调用 | `src/extraction/svelte_extractor.rs: extractTemplateCalls` 扫描模板中的 `{expression}` 块以获取调用模式 |
| Svelte `$state`/`$derived` 符文调用会产生噪音 | 符文是编译器内置函数，而不是真正的函数调用 | `src/extraction/svelte_extractor.rs` 从未解析的引用中过滤 `SVELTE_RUNES` 集 |
| 对象文字 getter/setter 提取为独立函数 | `object` 文字中的 `method_definition` 与类方法相同 | `src/extraction/tree_sitter/declarations.rs: extract_method` 跳过父节点为 `object`/`object_expression` 的 `method_definition` 节点 |
| JavaScript `class extends` 产生零继承边 | JS tree-sitter 使用 `class_heritage → identifier`（裸），而不是像 TypeScript 那样使用 `class_heritage → extends_clause → identifier` | `src/extraction/tree_sitter/type_refs.rs: extract_inheritance` — 当父级为 `class_heritage` 时，处理裸露的 `identifier`/`type_identifier` 子级 |
| PHP 特征提取为类 | `class_types` 中的 `trait_declaration` 但 `extract_class` 硬编码 `class` 类型 | `src/extraction/languages/php.rs: classify_class_node` 对于 `trait_declaration` 返回 `trait`； `src/extraction/tree_sitter_types.rs` 将 `trait` 添加到返回类型 |
| PHP 类属性缺失（0 个字段节点） | `extract_field` 寻找 `variable_declarator` 孩子； PHP 使用 `property_element > variable_name > name` | `src/extraction/tree_sitter/declarations.rs: extract_field` — 使用 `variable_name > name` 路径处理 `property_element` 子项 |
| PHP 类常量在类内跳过 | `variable_types` 检查有 `!is_inside_class_like_node()` 防护，因此 `const_declaration` 内部类失败 | `src/extraction/languages/php.rs: visit_node` 钩子捕获 `const_declaration`，提取 `const_element > name` 作为 `constant` 类型 |
| PHP `use TraitName` 类内不可见 | 类主体中的 `use_declaration` 节点未处理边 | `src/extraction/languages/php.rs: visit_node` 钩子从 `use_declaration` 中提取特征名称并创建 `implements` 未解析的引用 |

## 解决问题后

```bash
npm run build
rm -rf <codebase_path>/.rustcodegraph
target/release/rustcodegraph init -iv <codebase_path>
# Re-run the failing tests from above
```

在将语言标记为已验证之前，始终运行完整的测试套件：

```bash
npm test
```

## 添加 `get_receiver_type`

**仅适用于方法位于 AST 中顶级或其所有者类型之外的语言。**如果该语言将方法嵌套在类/结构体内（Python、Java、TypeScript、C#），则限定名称已包含父级 - 在添加任何内容之前进行完整性检查进行验证。

### 1.添加语言提取器的钩子

在 `src/extraction/languages/<lang>.rs` 中，为该语言的 `LanguageExtractor` 实现 `get_receiver_type`：

```rust
fn get_receiver_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
    // Extract the owner type name from the method's AST node.
    // Return Some(type_name) when the method should be qualified as:
    //   file_path::receiver_type::method_name
    let _ = (node, source);
    None
}
```

### 2.参考：Go实现

```rust
// src/extraction/languages/go.rs
fn get_receiver_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
    let receiver = get_child_by_field(node, "receiver")?;
    let text = get_node_text(receiver, source);
    receiver_name_from_go_receiver(&text)
}
```

### 3. 消费地点

`extract_method()` 中的 `src/extraction/tree_sitter/declarations.rs`：

```rust
if let Some(receiver) = receiver {
    let separator =
        if matches!(self.language, Language::Go | Language::Lua | Language::Luau) {
            "::"
        } else {
            "."
        };
    created.qualified_name = format!("{receiver}{separator}{name}");
}
```

## 关键文件

| 文件 | 角色 |
|------|------|
| `src/extraction/languages/<lang>.rs` | 语言提取器 — 节点类型、调用类型、`get_receiver_type` |
| `src/extraction/tree_sitter/declarations.rs` | 核心声明提取 — `extract_method()`、`extract_class()`、`extract_interface()` |
| `src/extraction/tree_sitter/calls.rs` | 核心调用提取 — `extract_call()` |
| `src/extraction/tree_sitter/type_refs.rs` | 核心继承与类型引用提取 — `extract_inheritance()` |
| `src/extraction/tree_sitter_types.rs` | `LanguageExtractor` trait 定义 |
| `src/search/query_utils.rs` | `STOP_WORDS`、搜索词提取、路径相关性评分 |
| `src/db/queries.rs` | FTS/BM25 符号搜索和精确名称辅助查询 |
| `src/context/index.rs` | `ContextBuilder` — 混合搜索+图遍历 |
| `src/mcp/tools.rs` | MCP 工具处理程序 — `rustcodegraph_explore` 实现 |

## 语言状态

### 已验证

- [x] **Go** — `get_receiver_type` 从 `func (sl *Type) method()` 中提取接收器
- [x] **Swift** — 不需要。树守护者将方法嵌套在类/扩展体内
- [x] **Java** — 不需要。方法嵌套在类体中。已针对 Guava 进行验证
- [x] **Python** — 不需要。方法嵌套在类体中。已针对 Flask 进行验证
- [x] **Rust** - `get_receiver_type` 走到父 `impl_item` 处以提取类型名称。还添加了从 struct 到 impl 方法的 `contains` 边。已针对 Deno 进行验证
- [x] **C** — 不需要。 C 中没有方法。强大的函数/结构/枚举提取，具有出色的调用边缘密度。针对 Redis 进行验证
- [x] **C++** — 仅头文件库不需要。 `is_misparsed_function` 钩子过滤器由宏引起的错误解析伪影（例如 `NLOHMANN_JSON_NAMESPACE_BEGIN`）。 `visit_function_body` 现在提取宏混乱的“函数”体内的结构节点（类/结构/枚举）。基于内容的 `.h` 检测（`grammars.rs` 中的 `looks_like_cpp`）将 C++ 标头提升为 `cpp` 语言，以便提取 `.h` 文件中的类。已针对 nlohmann/json 和 gRPC 进行验证。注意：类外 `Type::method()` 定义需要 `get_receiver_type`，但在仅标头代码库中并不常见。
- [x] **C#** — 不需要。方法嵌套在类体中。在 C# 的 `: Parent, IInterface` 语法的 `extract_inheritance` 中添加了 `base_list` 处理。添加了对 C# `property_declaration` 节点的 `property_types` 支持。修复了 `extract_field` 以处理 C# 的嵌套 `variable_declaration > variable_declarator` 结构。已针对 Jellyfin 进行验证
- [x] **Ruby** — `get_receiver_type` 不需要。方法嵌套在类体中。添加了 `visit_node` 挂钩，以使用适当的包含和限定名称提取 Ruby `module` 节点（关注点、命名空间）。模块内的方法获得 `Module::method` 限定名称。还将 `ExtractorContext` 与 `push_scope`/`pop_scope` 连接以用于语言挂钩。经话语验证
- [x] **TypeScript** — `get_receiver_type` 不需要。方法嵌套在类体中。将 `abstract_class_declaration` 添加到 `class_types`，以便正确提取抽象类。修复了单表达式箭头函数提取（`const fn = () => expr` 被悄悄删除，因为 `extract_name` 拾取了主体标识符，而不是返回 `<anonymous>` 进行父名称解析）。针对 Grafana 进行验证
- [x] **Dart** — `get_receiver_type` 不需要。方法嵌套在类体中。为基于选择器的方法调用添加了裸调用提取（例如 `object.method()`）。针对 Flutter 进行验证
- [x] **Kotlin** — `get_receiver_type` 从扩展函数 `fun Type.method()` 中提取接收器。添加了 `classify_class_node` 以区分接口/枚举和类（全部使用 `class_declaration` AST 节点）。添加了 `resolve_body` 钩子，因为 Kotlin 的树守护者语法不使用字段名称。添加了方法调用提取的 `navigation_expression` 处理。通过 `extra_class_node_types` 添加了 `object_declaration`。在 `extract_inheritance` 中为 Kotlin 的 `: Parent, Interface` 语法添加了 `delegation_specifier` 处理。还修复了 `extract_interface` 来访问身体子项（未提取接口方法）。添加了 `visit_node` 钩子来处理 `fun interface` (SAM) 声明 - tree-sitter-kotlin 不支持此 Kotlin 1.4+ 语法，会产生错误或 function_declaration 错误解析；该钩子检测这两种模式并提取接口。已针对 Koin、LeakCanary 进行验证
- [x] **Svelte** — 自定义 `SvelteExtractor` 将 `<script>` 块委托给 TS/JS 解析器；为每个 `.svelte` 文件创建 `component` 节点。添加了模板表达式调用提取：扫描标记中的 `{expression}` 块以查找函数调用（例如 `class={cn(...)}`），创建从组件到被调用者的调用边缘 - 将 Svelte 调用边缘从 29 增加到 387。过滤 Svelte 5 符文调用（`$state`、`$props`、`$derived`、`$effect`、`$bindable`）。还修复了：解构的 `$props()` 模式（例如 `let { x, y } = $props()`）不再提取为丑陋的多行变量名称（跳过 `extract_variable` 中的 `object_pattern`/`array_pattern`）。对象文字 getter/setter 方法不再提取为独立函数。已针对 shadcn-svelte 进行验证
- [x] **PHP** — `get_receiver_type` 不需要。方法嵌套在类体中。添加了 `classify_class_node` 以区分特征与类（`trait_declaration` → `trait` 类型）。将 `trait` 添加到 `tree_sitter_types.rs` 中的 `classify_class_node` 返回类型并在访问者中进行处理。修复了 PHP 属性提取：`extract_field` 现在可以处理 `property_element > variable_name > name` AST 结构（添加了 4,366 个字段节点）。添加了类常量的 `visit_node` 钩子（类内的 `const_declaration` 被 `variable_types` 防护跳过）和特征 `use` 声明（`use HasFactory, SoftDeletes;` 创建 `implements` 边 - 从 636 增加到 1,514）。已针对 Laravel 进行验证

### 需要验证

（目前没有）
