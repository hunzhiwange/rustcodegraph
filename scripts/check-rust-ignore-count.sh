#!/usr/bin/env bash
# 统计 Rust 相关测试中的 `ignore` 债务，供 CI 阻止新增忽略项。
# 主要输入是 `MAX_RUST_IGNORE_COUNT`；主要副作用是扫描 `tests/` 和 `src/`，超限时以非零状态退出。
set -euo pipefail

MAX_RUST_IGNORE_COUNT="${MAX_RUST_IGNORE_COUNT:-1}"
PATTERN='#\[ignore = "Rust'

if command -v rg >/dev/null 2>&1; then
  count="$(rg -n "$PATTERN" tests src 2>/dev/null | wc -l | tr -d ' ')"
else
  count="$(grep -R -n "$PATTERN" tests src 2>/dev/null | wc -l | tr -d ' ')"
fi

echo "Rust ignore count: ${count} (max ${MAX_RUST_IGNORE_COUNT})"

if [ "$count" -gt "$MAX_RUST_IGNORE_COUNT" ]; then
  echo "New Rust parity ignores were added. Remove them or lower the existing debt first." >&2
  exit 1
fi
