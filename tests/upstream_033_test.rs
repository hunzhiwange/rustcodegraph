#[path = "installer_targets_test/jsonc_preserving.rs"]
mod cases;
// This entry point reuses only the JSONC cases from the full installer suite.
// Keep its shared fixture intact, including the temporary home lifetime guard.
#[allow(unused_imports, dead_code)]
#[path = "installer_targets_test/common.rs"]
mod common;
