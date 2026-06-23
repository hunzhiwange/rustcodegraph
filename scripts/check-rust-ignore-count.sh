#!/usr/bin/env bash
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
