#!/usr/bin/env bash
#
# Assemble npm packages from Rust release archives.
#
# Produces, under release/npm/:
#   rustcodegraph-<target>/   per-platform package with bin/rustcodegraph(.exe)
#   main/                     rustcodegraph metadata package
#
# Supports cargo-dist archives for the rustcodegraph crate:
#   release/rustcodegraph-aarch64-apple-darwin.tar.xz
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${1:-$(node -p "require('$ROOT/package.json').version")}"
SCOPE="@colbymchenry"
PACKAGE_BASENAME="rustcodegraph"
REL="${RUSTCODEGRAPH_RELEASE_DIR:-$ROOT/release}"
NPM="$REL/npm"

rm -rf "$NPM"
mkdir -p "$NPM/main"

target_from_name() {
  case "$1" in
    rustcodegraph-darwin-arm64) echo "darwin-arm64" ;;
    rustcodegraph-darwin-x64) echo "darwin-x64" ;;
    rustcodegraph-linux-arm64) echo "linux-arm64" ;;
    rustcodegraph-linux-x64) echo "linux-x64" ;;
    rustcodegraph-win32-arm64) echo "win32-arm64" ;;
    rustcodegraph-win32-x64) echo "win32-x64" ;;
    rustcodegraph-aarch64-apple-darwin) echo "darwin-arm64" ;;
    rustcodegraph-x86_64-apple-darwin) echo "darwin-x64" ;;
    rustcodegraph-aarch64-unknown-linux-gnu) echo "linux-arm64" ;;
    rustcodegraph-x86_64-unknown-linux-gnu) echo "linux-x64" ;;
    rustcodegraph-aarch64-pc-windows-msvc) echo "win32-arm64" ;;
    rustcodegraph-x86_64-pc-windows-msvc) echo "win32-x64" ;;
    *) return 1 ;;
  esac
}

extract_archive() {
  local archive="$1"
  local dest="$2"
  case "$archive" in
    *.zip) tar -xf "$archive" -C "$dest" ;;
    *.tar.xz) tar -xJf "$archive" -C "$dest" ;;
    *.tar.gz) tar -xzf "$archive" -C "$dest" ;;
    *) echo "[pack-npm] unsupported archive: $archive" >&2; return 1 ;;
  esac
}

find_binary() {
  local dir="$1"
  local target="$2"
  local archive_base="$3"
  local binfile="rustcodegraph"
  if [[ "$target" == win32-* ]]; then
    binfile="rustcodegraph.exe"
  fi
  local candidates=(
    "$dir/$archive_base/bin/$binfile"
    "$dir/$archive_base/$binfile"
    "$dir/bin/$binfile"
    "$dir/$binfile"
  )
  local candidate
  for candidate in "${candidates[@]}"; do
    if [ -f "$candidate" ]; then
      echo "$candidate"
      return 0
    fi
  done
}

os_from_target() {
  echo "${1%-*}"
}

arch_from_target() {
  echo "${1##*-}"
}

shopt -s nullglob
archives=("$REL"/rustcodegraph-*.tar.gz "$REL"/rustcodegraph-*.tar.xz "$REL"/rustcodegraph-*.zip)
[ ${#archives[@]} -gt 0 ] || { echo "[pack-npm] no Rust bundles in $REL" >&2; exit 1; }

targets=()
seen=" "
for archive in "${archives[@]}"; do
  fname="$(basename "$archive")"
  base="$fname"
  base="${base%.tar.gz}"
  base="${base%.tar.xz}"
  base="${base%.zip}"
  target="$(target_from_name "$base" || true)"
  [ -n "$target" ] || continue
  case "$seen" in *" $target "*) continue ;; esac
  seen="$seen$target "

  os="$(os_from_target "$target")"
  arch="$(arch_from_target "$target")"
  binfile="$PACKAGE_BASENAME"
  [[ "$target" == win32-* ]] && binfile="$PACKAGE_BASENAME.exe"
  pkgdir="$NPM/$PACKAGE_BASENAME-$target"
  tmpx="$(mktemp -d)"
  mkdir -p "$pkgdir/bin"
  trap 'rm -rf "$tmpx"' RETURN
  extract_archive "$archive" "$tmpx"
  binary="$(find_binary "$tmpx" "$target" "$base")"
  [ -n "$binary" ] || { echo "[pack-npm] $fname did not contain rustcodegraph binary" >&2; exit 1; }
  cp "$binary" "$pkgdir/bin/$binfile"
  [[ "$target" == win32-* ]] || chmod +x "$pkgdir/bin/$binfile"
  for extra in README.md LICENSE CHANGELOG.md; do
    found="$(find "$tmpx" -type f -name "$extra" -print -quit)"
    [ -n "$found" ] && cp "$found" "$pkgdir/$extra"
  done
  [ -f "$ROOT/README.md" ] && [ ! -f "$pkgdir/README.md" ] && cp "$ROOT/README.md" "$pkgdir/README.md"
  [ -f "$ROOT/LICENSE" ] && [ ! -f "$pkgdir/LICENSE" ] && cp "$ROOT/LICENSE" "$pkgdir/LICENSE"

  VERSION="$VERSION" SCOPE="$SCOPE" PACKAGE_BASENAME="$PACKAGE_BASENAME" TARGET="$target" OSV="$os" ARCHV="$arch" BINFILE="$binfile" \
    node -e '
      const fs=require("fs");
      const bin = {};
      bin[process.env.PACKAGE_BASENAME] = `bin/${process.env.BINFILE}`;
      fs.writeFileSync(process.argv[1], JSON.stringify({
        name: `${process.env.SCOPE}/${process.env.PACKAGE_BASENAME}-${process.env.TARGET}`,
        version: process.env.VERSION,
        description: `RustCodeGraph native binary for ${process.env.TARGET}`,
        os: [process.env.OSV],
        cpu: [process.env.ARCHV],
        bin,
        files: ["bin", "README.md", "LICENSE", "CHANGELOG.md"],
        license: "MIT"
      }, null, 2) + "\n");
    ' "$pkgdir/package.json"

  rm -rf "$tmpx"
  trap - RETURN
  targets+=("$target")
  echo "[pack-npm] ${SCOPE}/${PACKAGE_BASENAME}-${target}@${VERSION}"
done

[ ${#targets[@]} -gt 0 ] || { echo "[pack-npm] no supported targets found in $REL" >&2; exit 1; }

[ -f "$ROOT/README.md" ] && cp "$ROOT/README.md" "$NPM/main/README.md"
[ -f "$ROOT/LICENSE" ] && cp "$ROOT/LICENSE" "$NPM/main/LICENSE"

VERSION="$VERSION" SCOPE="$SCOPE" PACKAGE_BASENAME="$PACKAGE_BASENAME" TARGETS="${targets[*]}" \
  node -e '
    const fs=require("fs");
    const opt={};
    for (const t of process.env.TARGETS.split(/\s+/).filter(Boolean))
      opt[`${process.env.SCOPE}/${process.env.PACKAGE_BASENAME}-${t}`]=process.env.VERSION;
    fs.writeFileSync(process.argv[1], JSON.stringify({
      name: `${process.env.SCOPE}/${process.env.PACKAGE_BASENAME}`,
      version: process.env.VERSION,
      description: "Local-first code intelligence for AI agents (MCP). Metadata package for platform-specific native CLI packages.",
      exports: {
        "./package.json": "./package.json"
      },
      optionalDependencies: opt,
      files: ["README.md", "LICENSE"],
      license: "MIT"
    }, null, 2) + "\n");
  ' "$NPM/main/package.json"

echo "[pack-npm] ${SCOPE}/${PACKAGE_BASENAME}@${VERSION} (${#targets[@]} platform packages in optionalDependencies; no root bin)"
echo "[pack-npm] output: $NPM"
