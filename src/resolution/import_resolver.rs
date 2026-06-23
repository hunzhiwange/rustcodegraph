//! Import path and import-binding resolution.
//!
//! 这一层把语言前端提取出的 import/require/use 统一成 `ImportMapping`，再由
//! 子模块按语言解析到文件或具体符号。公共 helper 保持无状态，方便 resolver
//! 缓存按文件粒度复用。

mod common;
mod cpp;
mod exports;
mod go;
mod jvm;
mod mappings;
mod path;
mod php;
mod python;
mod re_exports;
mod rust;
mod script_requires;
mod via;

pub use cpp::{clear_cpp_include_dir_cache, load_cpp_include_dirs};
pub use jvm::resolve_jvm_import;
pub use mappings::{clear_import_mapping_cache, extract_import_mappings};
pub use path::resolve_import_path;
pub use php::is_php_include_path_ref;
pub use re_exports::extract_re_exports;
pub use via::resolve_via_import;
