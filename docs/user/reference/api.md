# API

Use RustCodeGraph from Rust, or invoke the installed `rustcodegraph` CLI from another
runtime. npm packages are distribution packages only; they do not expose a
JavaScript API.

Rust callers can embed the library directly:

```rust
use rustcodegraph::{RustCodeGraph, IndexOptions};

let mut cg = RustCodeGraph::init_sync("/path/to/project")?;
cg.index_all(IndexOptions::default())?;

let results = cg.search_nodes("UserService", None);
let callers = cg.get_callers(&results[0].node.id);

cg.close();
```
