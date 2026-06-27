# 应用程序编程接口

使用 Rust 中的 RustCodeGraph，或从另一个调用已安装的 `rustcodegraph` CLI
运行时。 npm 包只是分发包；他们不暴露
JavaScript API。

Rust 调用者可以直接嵌入该库：

```rust
use rustcodegraph::{RustCodeGraph, IndexOptions};

let mut cg = RustCodeGraph::init_sync("/path/to/project")?;
cg.index_all(IndexOptions::default())?;

let results = cg.search_nodes("UserService", None);
let callers = cg.get_callers(&results[0].node.id);

cg.close();
```
