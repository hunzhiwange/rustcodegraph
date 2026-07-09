#!/bin/sh
#
# RustCodeGraph standalone installer.
#
# Downloads a native Rust binary bundle from GitHub Releases. No Node.js, no
# build tools, no npm required — ideal for a fresh Linux VPS over SSH.
#
#   curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh
#
# Upgrade:   run `rustcodegraph upgrade` (or just re-run the same command).
# Uninstall: curl -fsSL .../install.sh | sh -s -- --uninstall
#
# Environment:
#   RUSTCODEGRAPH_VERSION release tag to install (default: latest)
#   RUSTCODEGRAPH_INSTALL_DIR  bundle location   (default: ~/.rustcodegraph)
#   RUSTCODEGRAPH_BIN_DIR  symlink location  (default: ~/.local/bin)
set -eu

REPO="hunzhiwange/rustcodegraph"
INSTALL_DIR="${RUSTCODEGRAPH_INSTALL_DIR:-$HOME/.rustcodegraph}"
BIN_DIR="${RUSTCODEGRAPH_BIN_DIR:-$HOME/.local/bin}"

if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$BIN_DIR/rustcodegraph"
  rm -rf "$INSTALL_DIR"
  echo "RustCodeGraph uninstalled (removed $INSTALL_DIR and $BIN_DIR/rustcodegraph)."
  exit 0
fi

# 1. Detect platform -> target triple matching the release archives.
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Darwin) os="darwin" ;;
  Linux)  os="linux" ;;
  *) echo "rustcodegraph: unsupported OS '$os'." >&2; exit 1 ;;
esac
case "$arch" in
  arm64|aarch64) arch="arm64" ;;
  x86_64|amd64)  arch="x64" ;;
  *) echo "rustcodegraph: unsupported architecture '$arch'." >&2; exit 1 ;;
esac
target="${os}-${arch}"
case "$target" in
  darwin-arm64) artifact_target="aarch64-apple-darwin" ;;
  darwin-x64) artifact_target="x86_64-apple-darwin" ;;
  linux-arm64) artifact_target="aarch64-unknown-linux-gnu" ;;
  linux-x64) artifact_target="x86_64-unknown-linux-gnu" ;;
  *) echo "rustcodegraph: unsupported target '$target'." >&2; exit 1 ;;
esac

# 2. Resolve the version (latest release unless pinned).
#
# Resolve "latest" from the releases/latest *web* redirect, not the GitHub API:
# the unauthenticated API is rate-limited to 60 requests/hour per IP and returns
# 403 once exhausted — routine on shared/cloud hosts and CI (issue #325). The
# redirect (github.com/<repo>/releases/latest -> .../releases/tag/vX.Y.Z) has no
# such limit. Fall back to the API if the redirect can't be read.
version="${RUSTCODEGRAPH_VERSION:-}"
if [ -z "$version" ]; then
  version="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" \
    | sed -n 's#.*/releases/tag/##p')"
fi
if [ -z "$version" ]; then
  version="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)"
fi
[ -n "$version" ] || { echo "rustcodegraph: could not resolve latest version; set RUSTCODEGRAPH_VERSION (e.g. RUSTCODEGRAPH_VERSION=v0.9.4)." >&2; exit 1; }
# Release tags are vX.Y.Z; accept a bare X.Y.Z in RUSTCODEGRAPH_VERSION too.
case "$version" in v*) ;; *) version="v$version" ;; esac

# 3. Download + extract the bundle.
url="https://github.com/$REPO/releases/download/$version/rustcodegraph-${artifact_target}.tar.xz"
echo "Installing RustCodeGraph $version ($target)..."
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "$url" -o "$tmp/cg.tar.xz" || { echo "rustcodegraph: download failed: $url" >&2; exit 1; }

dest="$INSTALL_DIR/versions/$version"
rm -rf "$dest"
mkdir -p "$dest"
tar -xJf "$tmp/cg.tar.xz" -C "$dest"
mkdir -p "$dest/bin"
found="$dest/rustcodegraph-${artifact_target}/rustcodegraph"
[ -f "$found" ] || { echo "rustcodegraph: downloaded archive did not contain rustcodegraph-${artifact_target}/rustcodegraph" >&2; exit 1; }
mv "$found" "$dest/bin/rustcodegraph"
if [ ! -f "$dest/package.json" ]; then
  printf '{\n  "name": "rustcodegraph",\n  "version": "%s"\n}\n' "${version#v}" > "$dest/package.json"
fi
chmod +x "$dest/bin/rustcodegraph"

# 4. Symlink the launcher onto PATH and mark the current version.
mkdir -p "$BIN_DIR"
ln -sf "$dest/bin/rustcodegraph" "$BIN_DIR/rustcodegraph"
ln -sfn "$dest" "$INSTALL_DIR/current"

echo "Installed to $dest"
echo "Linked     $BIN_DIR/rustcodegraph"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *)
    echo ""
    echo "$BIN_DIR is not on your PATH. Add it:"
    echo "  export PATH=\"$BIN_DIR:\$PATH\""
    ;;
esac
echo ""
echo "Done. Run: rustcodegraph --help"
