//! CLI 子命令实现分组。
//!
//! project 负责会创建或变更索引状态的命令；query 负责只读查询与受影响文件分析。

pub(crate) mod project;
pub(crate) mod query;
