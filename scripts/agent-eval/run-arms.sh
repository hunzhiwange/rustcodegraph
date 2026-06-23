#!/usr/bin/env bash
# Tool-surface ablation — run ONE repo+question under ONE arm.
#
# Arms vary (exposed rustcodegraph tools, trace-first steering). Tools are trimmed
# SERVER-SIDE via RUSTCODEGRAPH_MCP_TOOLS in the MCP config's `env` block, so an
# ablated tool is genuinely absent from ListTools — no deferred-ToolSearch or
# denied-call confound (which --disallowedTools would introduce). Steering is
# injected with --append-system-prompt, so no rebuild of the shipped
# server-instructions is needed to A/B it.
#
#   A control       all tools            no steering
#   B steer         all tools            explore-flow first
#   C no-explore    hide explore         explore-flow first
#   D minimal       hide explore         explore-flow first
#   E control-probe hide explore         explore-flow first  (caller passes a NON-flow Q)
#
# Usage: run-arms.sh <repo-path> "<question>" <A|B|C|D|E> [run-id]
set -uo pipefail
REPO="${1:?repo path}"; Q="${2:?question}"; ARM="${3:?arm A-E}"; RID="${4:-1}"
CG_BIN="${RUSTCODEGRAPH_BIN:-$(command -v rustcodegraph)}"
OUT="${ARMS_OUT:-/tmp/arms}/$(basename "$REPO")"
mkdir -p "$OUT"
[ -n "$CG_BIN" ] || { echo "no rustcodegraph binary (set CG_BIN)"; exit 1; }
[ -d "$REPO/.rustcodegraph" ] || { echo "no .rustcodegraph index at $REPO"; exit 1; }

STEER='Flow questions ("how does X reach/become Y", "trace the flow", request to handler, state to render): call rustcodegraph_explore FIRST with the precise symbol names that span the flow. Use search only to locate endpoint symbols if you do not know them. Do NOT reconstruct the path with repeated search/callers.'
KEEP_NO_EXPLORE="search,node,callers,callees,impact,files,status"
KEEP_MINIMAL="search,node,callers,callees,impact,files,status"

case "$ARM" in
  A|G|H|I) TOOLS="";            STEERING="" ;;  # no steering; H = body-trace, I = body-trace + destination callees (sufficiency)
  B|F) TOOLS="";                STEERING="$STEER" ;;  # F = B's surface, run on the body-inlining trace build
  C) TOOLS="$KEEP_NO_EXPLORE";  STEERING="$STEER" ;;
  D|E) TOOLS="$KEEP_MINIMAL"; STEERING="$STEER" ;;
  *) echo "bad arm '$ARM' (want A|B|C|D|E)"; exit 1 ;;
esac

CFG="$OUT/mcp-$ARM.json"
if [ -n "$TOOLS" ]; then
  cat > "$CFG" <<JSON
{"mcpServers":{"rustcodegraph":{"command":"$CG_BIN","args":["serve","--mcp","--path","$REPO"],"env":{"RUSTCODEGRAPH_MCP_TOOLS":"$TOOLS"}}}}
JSON
else
  cat > "$CFG" <<JSON
{"mcpServers":{"rustcodegraph":{"command":"$CG_BIN","args":["serve","--mcp","--path","$REPO"]}}}
JSON
fi

LOG="$OUT/$ARM-r$RID.jsonl"; ERR="$OUT/$ARM-r$RID.err"
ARGS=( -p "$Q" --output-format stream-json --verbose
       --permission-mode bypassPermissions --model "${MODEL:-sonnet}" --effort "${EFFORT:-high}" --max-budget-usd 4
       --strict-mcp-config --mcp-config "$CFG" )
[ -n "$STEERING" ] && ARGS+=( --append-system-prompt "$STEERING" )

( cd "$REPO" && claude "${ARGS[@]}" > "$LOG" 2>"$ERR" )
echo "[$(basename "$REPO") $ARM r$RID] exit $? -> $LOG ($(wc -l < "$LOG" | tr -d ' ') lines)"
