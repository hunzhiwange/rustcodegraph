#!/usr/bin/env bash
# 把当前分支构建后临时链接成全局 `rustcodegraph`，便于本机手动验证。
# 主要输入是可选的 `--undo`；主要副作用是执行全局 `npm link/unlink` 并可能恢复已发布版本。
# Build the current branch and link it as the global `rustcodegraph` for
# hands-on testing. Replaces any existing global install for as long
# as the symlink is in place.
#
# Usage:
#   ./scripts/local-install.sh           # build + link
#   ./scripts/local-install.sh --undo    # unlink + restore the published version

set -euo pipefail

cd "$(dirname "$0")/.."

PKG=$(node -p "require('./package.json').name")
VERSION=$(node -p "require('./package.json').version")
BRANCH=$(git rev-parse --abbrev-ref HEAD)

if [ "${1:-}" = "--undo" ]; then
  echo "→ unlinking ${PKG}"
  npm unlink -g "${PKG}" >/dev/null 2>&1 || true
  echo "→ reinstalling published ${PKG}"
  npm install -g "${PKG}"
  echo "done: global rustcodegraph -> $(command -v rustcodegraph)"
  exit 0
fi

echo "→ building ${PKG} ${VERSION} (${BRANCH})"
npm run build

echo "→ linking globally"
npm link

LINKED=$(command -v rustcodegraph || echo "(not on PATH)")
echo
echo "✓ global rustcodegraph now points to this branch"
echo "  binary:  ${LINKED}"
echo "  branch:  ${BRANCH}"
echo "  version: ${VERSION}"
echo
echo "To restore the published version:"
echo "  ./scripts/local-install.sh --undo"
