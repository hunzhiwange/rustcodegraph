# 它是如何运作的

提取、存储、解析和自动同步管道。

RustCodeGraph 分四个阶段将源代码转换为可查询的图。

```
files → Extraction (tree-sitter) → DB (nodes/edges/files)
            ↓
      Resolution (imports, name-matching, framework patterns)
            ↓
      Graph queries (callers, callees, impact)
            ↓
      Context building (markdown / JSON for AI consumption)
```

## 1. 提取

[tree-sitter](https://tree-sitter.github.io/) 将源代码解析为 AST。特定于语言的查询提取 **节点**（函数、类、方法、类型...）和 **边缘**（调用、导入、扩展、实现）。繁重的解析在主线程中运行。

## 2. 储存

所有内容都会通过 FTS5 全文搜索存储到本地 SQLite 数据库 (`.rustcodegraph/rustcodegraph.db`) 中。 RustCodeGraph 在可用时使用本机 `better-sqlite3`，并透明地回退到 WASM 后端； `rustcodegraph status` 显示哪个是实时的。

## 3. 分辨率

提取后，引用被解析：函数调用→定义、导入→源文件、类继承和特定于框架的模式。 一些动态调度边界（回调、观察者、React 重新渲染、JSX 子级）由合成器桥接，因此流可以端到端连接。 请参阅[解析与框架](./resolution.md)。

## 4.自动同步

MCP 服务器使用本机操作系统文件事件（FSEvents / inotify / ReadDirectoryChangesW）监视您的项目。 更改会被反跳、过滤到源文件并增量同步 - 图表在您编码时保持新鲜，无需配置。
