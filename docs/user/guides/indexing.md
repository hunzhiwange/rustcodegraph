# 索引项目

完整索引、增量同步和索引新鲜度。

## 初始化和索引

```bash
cd your-project
rustcodegraph init -i      # 初始化并完整索引
```

`init` 创建 `.rustcodegraph/`；`-i`/`--index` 会立即构建索引。只想先初始化时，可以省略该标志，稍后再运行 `rustcodegraph index`。

## 完整与增量

```bash
rustcodegraph index           # 完整索引整个项目
rustcodegraph index --force   # 从头重建索引
rustcodegraph sync            # 增量同步，只处理变更文件
```

`sync` 速度很快，因为它只修复更改的内容。在分支切换或批量编辑后使用它。

## 保持索引新鲜

RustCodeGraph 的 CLI 不会假设有长期运行的后台服务。代码变更后，先运行一次增量同步，再依赖搜索、调用关系或影响分析结果：

```bash
rustcodegraph sync
```

建议在这些时机同步：

- **切换分支或执行 `git pull` 后。** 工作树可能已经和索引不一致。
- **批量编辑、代码生成或格式化后。** 一次 `sync` 会合并处理所有变化。
- **让代理继续分析前。** 如果你知道刚刚改过代码，先同步能避免代理基于旧图回答。
- **CI 或脚本开始时。** 脚本里第一步运行 `rustcodegraph sync`，后续 `query`、`explore`、`affected` 才会看见当前工作树。

`sync` 会扫描文件状态并只更新发生变化的部分；需要完全重建时才使用 `rustcodegraph index --force`。

当前推荐路径是直接使用 CLI 和本技能：初始化一次，代码变化后运行 `rustcodegraph sync`，再调用 `explore`、`node`、`query`、`callers`、`callees`、`impact` 或 `affected`。

## 检查状态

```bash
rustcodegraph status
```

报告节点、边、文件计数、活动 SQLite 后端和日志模式。`status` 只读取当前索引状态，不会替你同步；如果刚改过代码，请先运行 `rustcodegraph sync`。

## 什么被索引

每个扩展名映射到[受支持语言](../reference/languages.md)的文件都会被索引，但默认排除依赖/构建目录（`node_modules`、`vendor`、`dist` 等）、`.gitignore` 排除的内容以及超过 1 MB 的文件。请参阅[配置](../getting-started/configuration.md)。
