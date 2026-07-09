# MCP服务器

RustCodeGraph通过MCP向AI代理公开的工具。

RustCodeGraph 作为[模型上下文协议 (MCP)](https://modelcontextprotocol.io/)服务器运行。开始时间：

```bash
rustcodegraph serve --mcp
```

安装程序配置的代理会自动启动。存在`.rustcodegraph/`索引时，客服代表使用以下工具。

## 工具

| 工具 | 目的 |
|---|---|
| `rustcodegraph_search` | 在代码库中按名称查找符号 |
| `rustcodegraph_callers` | 查找调用函数的内容 |
| `rustcodegraph_callees` | 查找函数调用的内容 |
| `rustcodegraph_impact` | 分析更改符号会影响哪些代码 |
| `rustcodegraph_node` | 获取特定符号的详细信息（可选择使用源代码） |
| `rustcodegraph_explore` | 在一次调用中返回按文件分组的几个相关符号的源，以及关系映射 |
| `rustcodegraph_files` | 获取索引文件结构（比文件系统扫描更快） |
| `rustcodegraph_status` | 检查索引健康状况和统计数据 |

## 客服代表应如何使用该功能

RustCodeGraph *是*预构建的搜索索引。对于“X如何工作？”、架构、跟踪或where-is-X问题，代理应该在几个RustCodeGraph调用中回答并停止—通常使用* *零文件读取* * —而不是使用`grep` + `Read`重新导出答案。RustCodeGraph的直接答案是几次调用； grep/read探索是几十次。

安装程序会自动将此指南写入每个代理的说明文件。
