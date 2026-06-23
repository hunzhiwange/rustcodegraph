//! Multi-target installer tests.
//!
//! This is the Rust port of `__tests__/installer-targets.test.ts`. The Rust
//! package, binary, MCP key, and generated instructions all use `rustcodegraph`.
//! Assertions preserve filesystem, idempotency, cleanup, and registry paths.

#[path = "installer_targets_test/common.rs"]
mod common;

#[path = "installer_targets_test/cursor_rules_file_cleanup_on_uninstall.rs"]
mod installer_cursor_rules_file_cleanup_on_uninstall;
#[path = "installer_targets_test/claude_partial_state_idempotency.rs"]
mod installer_targets_claude_partial_state_idempotency;
#[path = "installer_targets_test/contract.rs"]
mod installer_targets_contract;
#[path = "installer_targets_test/opencode_xdg_config_path_535.rs"]
mod installer_targets_opencode_xdg_config_path_535;
#[path = "installer_targets_test/partial_state_idempotency.rs"]
mod installer_targets_partial_state_idempotency;
#[path = "installer_targets_test/registry.rs"]
mod installer_targets_registry;
#[path = "installer_targets_test/remaining_partial_state_idempotency.rs"]
mod installer_targets_remaining_partial_state_idempotency;
#[path = "installer_targets_test/toml_serializer_codex_backbone.rs"]
mod installer_targets_toml_serializer_codex_backbone;
#[path = "installer_targets_test/uninstall_targets_sweep.rs"]
mod installer_uninstall_targets_sweep;
