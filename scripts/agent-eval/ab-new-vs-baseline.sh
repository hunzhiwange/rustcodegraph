#!/usr/bin/env bash
# A/B a codegraph retrieval/steering change: the NEW build (current HEAD) vs a
# BASELINE build (a git ref) — BOTH with rustcodegraph attached — on the same
# implementation task, measuring how many Read vs rustcodegraph calls the agent
# makes. ISOLATES the change (unlike run-all.sh's with-vs-without). The agent
# works on a throwaway copy of the target, so your repos are never touched.
#
# Reliable attach (works even when this is itself run nested inside a Claude
# session): each arm PRE-WARMS a persistent rustcodegraph daemon for its target so
# claude connects to an already-bound, index-loaded daemon instantly — before
# the agent's first turn. Without this, on a multi-step task the agent
# dives into Read/grep before rustcodegraph finishes its ~2-3s startup (worse under
# the CPU contention of a nested run) and runs with NO rustcodegraph.
#
# Gotcha: claude's `system/init` snapshot can read status:"pending" / 0 tools
# even when the server then connects fine — judge by ACTUAL rustcodegraph usage in
# `rustcodegraph agent-eval parse-run`'s "by type", not the init line.
#
# Usage: ab-new-vs-baseline.sh <indexed-repo> "<task>" [baseline-ref]
#   <indexed-repo>  a repo with a .rustcodegraph index (copied per arm)
#   "<task>"        an implementation task, e.g. "Add X to Y and wire it through"
#   [baseline-ref]  git ref for the BEFORE build (default: HEAD~1)
# Env: AGENT_EVAL_OUT (default: /tmp/ab-new-vs-baseline)
set -euo pipefail

TARGET="${1:?usage: ab-new-vs-baseline.sh <indexed-repo> \"<task>\" [baseline-ref]}"
TASK="${2:?task required}"
BASE_REF="${3:-HEAD~1}"
ENGINE="$(cd "$(dirname "$0")/../.." && pwd)"
OUT_ROOT="${AGENT_EVAL_OUT:-/tmp/ab-new-vs-baseline}"
mkdir -p "$OUT_ROOT"
OUT=$(mktemp -d "$OUT_ROOT/run.XXXXXX")
PARSER_BIN="$OUT/codegraph-agent-eval-parser"

command -v claude >/dev/null || { echo "claude CLI not on PATH"; exit 1; }
[ -d "$TARGET/.rustcodegraph" ] || { echo "target not indexed: run 'rustcodegraph init $TARGET' first"; exit 1; }
DAEMON_PID=""
SLOT_LOCK=""
SNAPSHOT=""
stop_daemon() {
  if [ -n "$DAEMON_PID" ]; then
    kill -KILL "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
    DAEMON_PID=""
  fi
}
release_slot() {
  if [ -n "$SLOT_LOCK" ]; then
    rm -f "$SLOT_LOCK/owner"
    rmdir "$SLOT_LOCK"
    SLOT_LOCK=""
  fi
}
cleanup() {
  stop_daemon
  release_slot
  [ -z "$SNAPSHOT" ] || rm -rf "$SNAPSHOT"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

build_ref() {
  local ref="$1" label="$2" n
  SNAPSHOT=$(mktemp -d)
  git -C "$ENGINE" archive "$ref" | tar -x -C "$SNAPSHOT"
  mkdir -p "$ENGINE/target/rust-slot-locks"
  while [ -z "$SLOT_LOCK" ]; do
    for n in {1..10}; do
      if mkdir "$ENGINE/target/rust-slot-locks/slot-$n.lock" 2>/dev/null; then
        SLOT_LOCK="$ENGINE/target/rust-slot-locks/slot-$n.lock"
        printf 'A/B task1 pid=%s ref=%s\n' "$$" "$ref" > "$SLOT_LOCK/owner"
        export CARGO_TARGET_DIR="$ENGINE/target/rust-slot-$n"
        # Keep intermediate artifacts in the locked slot even when the archived
        # ref configures a separate build.build-dir in .cargo/config.toml.
        export CARGO_BUILD_BUILD_DIR="$CARGO_TARGET_DIR"
        break
      fi
    done
    [ -n "$SLOT_LOCK" ] || sleep 1
  done
  # Archive timestamps can be older than the previous build in this slot.
  # Invalidate this package so Cargo cannot reuse the other ref's binary.
  (cd "$SNAPSHOT" && cargo clean --release -p rustcodegraph && cargo build --release --bin rustcodegraph) > "$OUT/build-$label.log" 2>&1
  cp "$CARGO_TARGET_DIR/release/rustcodegraph" "$OUT/rustcodegraph-$label"
  release_slot
  rm -rf "$SNAPSHOT"
  SNAPSHOT=""
}

mkdir -p "$OUT"
echo "###### engine=$ENGINE  baseline=$BASE_REF"

echo "###### target=$TARGET"
echo "###### task=$TASK"
echo

# Two pristine copies so each arm starts clean (the agent edits its own copy).
rm -rf "$OUT/t-new" "$OUT/t-base"
rsync -a --exclude node_modules --exclude .git --exclude dist --exclude .rustcodegraph "$TARGET/" "$OUT/t-new/"
cp -R "$OUT/t-new" "$OUT/t-base"

prewarm() { # target — spawn a persistent daemon (current $BIN) and wait for its socket
  RUSTCODEGRAPH_DAEMON_INTERNAL=1 RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS=1800000 "$BIN" serve --mcp --path "$1" </dev/null >/dev/null 2>&1 &
  DAEMON_PID=$!
  node -e 'const fs=require("fs");let n=0;const t=setInterval(()=>{if(fs.existsSync(process.argv[1]+"/.rustcodegraph/daemon.sock")){clearInterval(t);process.exit(0)}if(n++>150){clearInterval(t);process.exit(1)}},100)' "$1" \
    && echo "  daemon warm: $1" || echo "  WARN: daemon never bound for $1 (arm may run without codegraph)"
}

run_arm() { # label, target-copy
  local label="$1" tgt="$2" c="$OUT/mcp-$1.json"
  # Connect to the pre-warmed daemon for a fast attach.
  printf '{"mcpServers":{"rustcodegraph":{"command":"%s","args":["serve","--mcp","--path","%s"]}}}' "$BIN" "$tgt" > "$c"
  prewarm "$tgt"
  echo "############## ARM [$label] ##############"
  ( cd "$tgt" && claude -p "$TASK" \
      --output-format stream-json --verbose --permission-mode bypassPermissions \
      --model "${MODEL:-sonnet}" --effort "${EFFORT:-high}" --max-budget-usd 4 --strict-mcp-config --mcp-config "$c" \
      </dev/null > "$OUT/run-$label.jsonl" 2>"$OUT/run-$label.err" )
  "$PARSER_BIN" agent-eval parse-run "$OUT/run-$label.jsonl" 2>&1 | grep -E "by type|Result" || echo "  (parse failed — see $OUT/run-$label.jsonl)"
  stop_daemon
  echo
}

build_ref HEAD new
build_ref "$BASE_REF" baseline
BIN="$OUT/rustcodegraph-new"
cp "$BIN" "$PARSER_BIN"
"$BIN" init "$OUT/t-new"
run_arm new "$OUT/t-new"
BIN="$OUT/rustcodegraph-baseline"
"$BIN" init "$OUT/t-base"
run_arm baseline "$OUT/t-base"

echo "###### DONE. Compare the [new] vs [baseline] 'by type' counts above"
echo "###### (especially Read vs mcp__rustcodegraph__*). Full logs in: $OUT"
