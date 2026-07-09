# 知识图谱

构建图的节点和边类型。

RustCodeGraph 存储三样东西：**节点**（符号和文件）、**边**（它们之间的关系）和**文件**。 每个节点和边都带有一个精确的 `kind`，该 `kind` 取自固定词汇表，因此跨语言的查询是一致的。

## 节点种类

`file`、`module`、`class`、`struct`、`interface`、`trait`、`protocol`、`function`、`method`、`property`、`field`、`variable`、`constant`、`enum`、 `enum_member`、`type_alias`、`namespace`、`parameter`、`import`、`export`、`route`、`component`。

## 边缘种类

`contains`、`calls`、`imports`、`exports`、`extends`、`implements`、`references`、`type_of`、`returns`、`instantiates`、`overrides`、`decorates`。

## 出处

大多数边直接来自 AST。 其中一些（在静态解析无法遵循的动态分派边界处）被**合成**并标记为 `provenance: 'heuristic'` 以及创建它们的接线站点。 这些内容在 `explore` 和 `node` 路径中内联显示，因此代理可以准确地看到连接来自何处。

## 查询它

- **按名称搜索**符号 (FTS5)。 
- **调用者/被调用者** 一次一跳地遍历调用图。 
- **影响** 计算受更改影响的传递半径。 
- **Explore** 在一次调用中返回按文件分组的多个相关符号的源，以及它们之间的调用路径。

有关如何运行它们的信息，请参阅[命令行界面](../reference/cli.md)和 [MCP 服务器](../reference/mcp-server.md)参考资料。
