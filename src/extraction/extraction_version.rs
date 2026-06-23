//! Extraction version parity with `src/extraction/extraction-version.ts`.
//!
//! Bump this whenever extraction semantics change in a way that should force
//! re-indexing of existing databases.
//!
//! 这个版本号进入项目 metadata；当节点/边语义改变时提升它，旧索引会被
//! 视为需要重建，而普通重构或注释变更不应该改动。

pub const EXTRACTION_VERSION: u32 = 24;
