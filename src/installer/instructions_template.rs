//! Marker-fenced agent instructions block.
//!
//! A short block is written into agent instruction files so subagents and non-MCP harnesses know RustCodeGraph
//! exists, without duplicating the full MCP initialize instructions.
//!
//! 这里的块必须短：它会进入各类 agent 的长期说明文件，只负责提示
//! “已索引时先用 RustCodeGraph”，完整工具选择策略仍以 MCP initialize
//! 返回的 server instructions 为准。

// 这些标记是卸载和重复安装的唯一锚点；改动时要同步所有 target 测试。
pub const RUSTCODEGRAPH_SECTION_START: &str = "<!-- RUSTCODEGRAPH_START -->";
pub const RUSTCODEGRAPH_SECTION_END: &str = "<!-- RUSTCODEGRAPH_END -->";

pub const RUSTCODEGRAPH_INSTRUCTIONS_BLOCK: &str = concat!(
    "<!-- RUSTCODEGRAPH_START -->\n",
    "## RustCodeGraph\n\n",
    "In repositories indexed by RustCodeGraph (a `.rustcodegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:\n\n",
    "- **MCP tools** (when available): `rustcodegraph_explore` answers most code questions in one call - the relevant symbols' verbatim source plus the call paths between them. `rustcodegraph_node` returns one symbol's source + callers, or reads a whole file with line numbers. If the tools are listed but deferred, load them by name via tool search.\n",
    "- **Shell** (always works): `rustcodegraph explore \"<symbol names or question>\"` and `rustcodegraph node <symbol-or-file>` print the same output.\n\n",
    "If there is no `.rustcodegraph/` directory, skip RustCodeGraph entirely - indexing is the user's decision.\n",
    "<!-- RUSTCODEGRAPH_END -->",
);
