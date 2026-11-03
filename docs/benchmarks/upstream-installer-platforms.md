# Upstream installer platform acceptance

## macOS — 2026-11-03

Source reference: local upstream `codegraph@15483713`; implementation baseline `4bb5e511`.
Host: native Darwin. All configuration writes use fixture-owned temporary HOME and cwd; real agent settings are untouched.

`case_059_macos_paths_and_roundtrips` checks twelve targets: Copilot VS Code, Copilot CLI, Copilot JetBrains, Bob, Droid, Windsurf, Qwen, DSH, Grok, Kimi, Qoder, and Pi. It asserts their exact macOS global configuration paths (including VS Code's Application Support directory), fresh detection without writes, successful install/uninstall, byte-identical repeat install, and harmless repeat uninstall. Existing target suites additionally cover local/global support, overrides, malformed files and preservation of user settings.

The new assertions passed immediately: existing behavior was sufficient, so no artificial failing behavior or product change was introduced.

Validation:

- `cargo fmt --all`: exit 0.
- `cargo test -p rustcodegraph --test installer_targets_test -- --nocapture --test-threads=1`: exit 0; 163 passed, 0 failed, 0 ignored, 0 filtered out (including case_059).
- Shared slot 1; `CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1`. Owner record and lock released after all processes exited; compilation cache retained.
- Raw execution output: `/tmp/rcg-059-test.log` (temporary local evidence).

The twelve prerequisite roundtrip implementation commits were present and their tests ran in this suite. Qoder/Pi installation tasks retain separate cross-platform regression notes; those do not block this macOS acceptance. Linux and Windows acceptance remain separate tasks; this report claims no results for those platforms.

## Linux — 2026-11-03

Source: codegraph@15483713; implementation baseline 64e96da2. Docker node:22-bookworm, aarch64, native Rust stable 1.99.1, init enabled. Source copied to /repo excluding target, target-dir, .git, node_modules and .rustcodegraph. Fixture-owned temporary HOME isolates all settings.

case_060_linux_paths_and_roundtrips checks all twelve targets listed above, including Linux VS Code .config/Code/User/mcp.json. It verifies exact global paths, detection without writes, install/uninstall, byte-identical repeat installation and harmless repeated uninstall. Assertions passed immediately; no artificial red phase or product change was needed.

- cargo fmt --all: exit 0.
- cargo test -p rustcodegraph --test installer_targets_test -- --nocapture --test-threads=1: exit 0; 163 passed, 0 failed, 0 ignored, 0 filtered, including case_060.
- Dynamically acquired container repository slot 1; CARGO_TARGET_DIR=/repo/target/rust-slot-1. Trap released owner record and lock after command completion; cache retained.
- Raw log: /tmp/rcg-060-linux-test.log (temporary local evidence). Minimal toolchain initially lacked rustfmt; installing that component resolved the format prerequisite before the successful suite.
- No Windows acceptance is claimed.
