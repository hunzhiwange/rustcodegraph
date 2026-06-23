#!/usr/bin/env bash
#
# Build a native RustCodeGraph bundle for one platform.
#
# Usage:
#   scripts/build-bundle.sh <target>
#     target: darwin-arm64 | darwin-x64 | linux-x64 | linux-arm64
#           | win32-x64 | win32-arm64
#
# Output:
#   unix:    release/rustcodegraph-<target>.tar.gz   (binary: bin/rustcodegraph)
#   windows: release/rustcodegraph-<target>.zip      (binary: bin/rustcodegraph.exe)
set -euo pipefail

TARGET="${1:?usage: build-bundle.sh <target>}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/release"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

case "$TARGET" in
  darwin-arm64) RUST_TARGET="aarch64-apple-darwin" ;;
  darwin-x64) RUST_TARGET="x86_64-apple-darwin" ;;
  linux-arm64) RUST_TARGET="aarch64-unknown-linux-gnu" ;;
  linux-x64) RUST_TARGET="x86_64-unknown-linux-gnu" ;;
  win32-arm64) RUST_TARGET="aarch64-pc-windows-msvc" ;;
  win32-x64) RUST_TARGET="x86_64-pc-windows-msvc" ;;
  *) echo "[bundle] unsupported target: $TARGET" >&2; exit 1 ;;
esac

echo "[bundle] target=${TARGET} rust=${RUST_TARGET}"

echo "[bundle] building Rust binary"
( cd "$ROOT" && cargo build --release --locked --bin rustcodegraph --target "$RUST_TARGET" )

BUILT_BIN="rustcodegraph"
BIN="rustcodegraph"
if [[ "$TARGET" == win32-* ]]; then
  BUILT_BIN="rustcodegraph.exe"
  BIN="rustcodegraph.exe"
fi
BUILT="$ROOT/target/$RUST_TARGET/release/$BUILT_BIN"
[ -f "$BUILT" ] || { echo "[bundle] error: binary not found ($BUILT)" >&2; exit 1; }

STAGE="$WORK/rustcodegraph-${TARGET}"
mkdir -p "$STAGE/bin"
cp "$BUILT" "$STAGE/bin/$BIN"
[[ "$TARGET" == win32-* ]] || chmod +x "$STAGE/bin/$BIN"
cp "$ROOT/package.json" "$STAGE/package.json"
[ -f "$ROOT/README.md" ] && cp "$ROOT/README.md" "$STAGE/README.md"
[ -f "$ROOT/LICENSE" ] && cp "$ROOT/LICENSE" "$STAGE/LICENSE"

mkdir -p "$OUT"
if [[ "$TARGET" == win32-* ]]; then
  ARCHIVE="$OUT/rustcodegraph-${TARGET}.zip"
  rm -f "$ARCHIVE"
  ( cd "$WORK" && zip -rqX "$ARCHIVE" "rustcodegraph-${TARGET}" )
else
  ARCHIVE="$OUT/rustcodegraph-${TARGET}.tar.gz"
  tar --no-xattrs -czf "$ARCHIVE" -C "$WORK" "rustcodegraph-${TARGET}"
fi
echo "[bundle] wrote ${ARCHIVE} ($(du -h "$ARCHIVE" | cut -f1))"
