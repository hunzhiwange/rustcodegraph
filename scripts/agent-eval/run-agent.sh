#!/usr/bin/env bash
# 对单个仓库发起一次无头 Claude 评估运行，并保存完整 JSONL 事件流用于后续统计。
# 主要输入是仓库路径、标签和提示词；主要副作用是启动带 RustCodeGraph MCP 的 `claude` 进程，并把输出写到 `AGENT_EVAL_OUT`。
# Headless Claude Code run against a repo with rustcodegraph MCP, capturing the
# full stream-json so we can see tool calls + token usage. Complements the
# interactive itrun.sh: headless gives a clean per-tool breakdown + exact
# tokens/cost, but defaults to the general-purpose subagent (not Explore).
# To force the Explore path, ask for it in the prompt.
#
# Usage: run-agent.sh <repo-path> <label> "<prompt>"
# Env: AGENT_EVAL_OUT (default /tmp/agent-eval), CG_BIN (codegraph Rust binary)
set -uo pipefail

REPO="$1"; LABEL="$2"; PROMPT="$3"
CG_BIN="${CG_BIN:-$(command -v rustcodegraph || echo /usr/local/bin/rustcodegraph)}"
OUT_DIR="${AGENT_EVAL_OUT:-/tmp/agent-eval}"; mkdir -p "$OUT_DIR"
OUT="$OUT_DIR/run-${LABEL}.jsonl"

MCP_CONFIG=$(cat <<JSON
{"mcpServers":{"rustcodegraph":{"command":"${CG_BIN}","args":["serve","--mcp","--path","${REPO}"]}}}
JSON
)

echo "→ running [$LABEL] in $REPO"
cd "$REPO" || exit 1

claude -p "$PROMPT" \
  --output-format stream-json --verbose \
  --permission-mode bypassPermissions \
  --model "${MODEL:-sonnet}" --effort "${EFFORT:-high}" \
  --max-budget-usd 2 \
  --strict-mcp-config --mcp-config "$MCP_CONFIG" \
  > "$OUT" 2>"$OUT_DIR/run-${LABEL}.err"

echo "exit: $? | wrote $OUT ($(wc -l < "$OUT") lines)"
"$CG_BIN" agent-eval parse-run "$OUT" 2>/dev/null || true
