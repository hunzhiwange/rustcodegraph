#!/usr/bin/env bash
# 这是 Claude 的 PreToolUse 实验钩子，用来阻止读取已索引源码并把代理引导到 RustCodeGraph 工具。
# 主要输入是 hook 传入的 JSON；主要副作用是对源码文件返回拒绝决定，对其它文件保持放行。
# PreToolUse hook (experiment): deny Read of codegraph-indexed source files and
# steer the agent to rustcodegraph_explore/rustcodegraph_node instead. Tests whether
# rustcodegraph can FULLY replace Read for code-understanding once the escape hatch
# is removed. Non-source reads (config, .env, markdown, new files) pass through.
#
# Wire by generating a temporary settings JSON that points at this hook's
# absolute path, for example:
#   HOOK="$(cd "$(dirname "$0")" && pwd)/block-read-hook.sh"
#   jq -n --arg cmd "bash $HOOK" \
#     '{hooks:{PreToolUse:[{matcher:"Read",hooks:[{type:"command",command:$cmd}]}]}}'
set -uo pipefail
input="$(cat)"
fp="$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty' 2>/dev/null)"

case "$fp" in
  *.ts|*.tsx|*.js|*.jsx|*.mjs|*.cjs|*.py|*.go|*.rs|*.java|*.rb|*.php|*.swift|*.kt|*.kts|*.c|*.cc|*.cpp|*.h|*.hpp|*.cs|*.lua|*.vue|*.svelte)
    msg="Read is disabled for source files in this session — rustcodegraph already has this file indexed (with line numbers, kept in sync on every change). Use rustcodegraph_explore (several related symbols at once) or rustcodegraph_node (one symbol's full source). If a symbol you need wasn't in a prior explore, run ANOTHER rustcodegraph_explore with its exact name instead of reading the file."
    jq -n --arg m "$msg" '{reason:$m, hookSpecificOutput:{hookEventName:"PreToolUse",permissionDecision:"deny",permissionDecisionReason:$m}}'
    exit 0
    ;;
esac
exit 0
