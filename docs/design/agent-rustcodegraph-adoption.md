# Getting agents to actually use rustcodegraph (not Read) — design notes & handoff

> Working doc for a fresh session. Two problems to crack:
> **(P1)** agents still reach for `Read`/`grep` during implementation instead of rustcodegraph;
> **(P2)** on startup the rustcodegraph MCP server can be `pending` when the agent's first turn fires, so the agent runs with *no* rustcodegraph at all.
>
> Read `rustcodegraph/CLAUDE.md` → "Retrieval performance & dynamic-dispatch coverage" first — it's the doctrine these ideas must respect.

---

## Context — what already shipped (so you don't repeat it)

- **#733 (`7175dc4`)** — reframed the agent-facing steering (`src/mcp/server_instructions.rs` + the `rustcodegraph_node`/`rustcodegraph_explore` descriptions in `src/mcp/tools.rs`) to cover *implementation*, not just Q&A; and added **file-view mode**: `rustcodegraph_node` now accepts a bare `file` (no `symbol`) → returns that file's symbol map + its dependents (blast radius) + verbatim bodies (`includeCode`).
- **Clean A/B result** (new build vs baseline build, both rustcodegraph-connected, same fully-implemented task — `kindExclude` added to `rustcodegraph_search`):
  - **baseline:** 0 rustcodegraph calls, 8 Reads (agent *ignored* available rustcodegraph).
  - **new:** 2 `rustcodegraph_explore` calls, 5 Reads.
  - So the reframe *did* move tool-choice — but the agent used `rustcodegraph_explore`, **never the file-view**, and still Read 5×. n=1/arm.
- **Eval harness fix** (`#735`): nested attach is a *startup-latency* problem, not a hard block. `scripts/agent-eval/ab-new-vs-baseline.sh` now pre-warms a daemon + skips the re-exec; use it (run non-nested for cleanest results).

**Doctrine constraints (from CLAUDE.md — do not relitigate):**
- *Adapt the tool to the agent.* Changing tool descriptions / `server_instructions.rs` is **low-salience** and has *regressed* wall-clock before. Wording alone won't reliably move tool-choice.
- *New tools fare worse than extending an existing one* (the agent under-picks even `trace`; `context` was removed).
- The real levers that landed historically: **coverage** (more flows connect statically → `explore` surfaces them) and **sufficiency** (output complete enough that the agent *stops* reading).
- The optimization target is **wall-clock + tool-call count + Read=0**, not token cost (cost is lower as a side effect).

---

## P1 — Agents under-use rustcodegraph during implementation

### STATUS — 2026-06-08 (RESOLVED via Read-parity, not a hook)

**The fix: make `rustcodegraph_node` read a file *exactly like the Read tool*, only
faster — so the agent reaches for it naturally. No forcing.** The owner's steer
settled the direction: *"rustcodegraph should be able to Read just like the Read
tool… make it as good as Read. Read is slow and old; querying the index is fast.
You keep diverging away from using rustcodegraph rather than pursuing the fix."*

**DONE — file-view handling in `src/mcp/tools.rs` is now full Read parity:**
- A `file` with no `symbol` returns the file's current source numbered
  **byte-for-byte the way Read does — `<n>\t<line>`, no padding, trailing empty
  line kept** (verified by reading the same file with both and diffing). The only
  addition is a **one-line blast-radius header** (`used by N files: …`).
- **`offset` / `limit` mean exactly what they do on Read** (1-based start; max
  lines; default whole file capped at 2000 lines like Read). Large files paginate
  honestly (`(lines X–Y of N — pass offset/limit…)`), never the 15k `truncateOutput` chop.
- Content is the **default** (no `includeCode` needed); `symbolsOnly: true` returns
  the cheap structural map instead. Security preserved: `yaml`/`properties`
  summarized by key, never dumped (#383); reads via `validatePathWithinRoot` (#527).
- Tests: `tests/node_file_view_test.rs` (incl. strict format parity
  `^1000\t  const v998 = 998;` and unpadded `^1\timport …`). Full suite green
  (1270). Descriptions / `server_instructions.rs` / CHANGELOG reframed: "read a
  source file with rustcodegraph_node instead of Read — same bytes, faster."

**The hook (idea 1) — A/B'd and REJECTED. Do not ship.** Kept only as an eval
artifact (`scripts/agent-eval/redirect-read-hook.sh` + `ab-hook.sh`).
- Clean A/B (2 runs/arm, devpit "add `dp ping`, build it"; both arms rustcodegraph-attached):
  - **nohook:** 0 rustcodegraph calls, 1 Read, **5–7 tool calls, 6–8 turns, 55–77s.** (Reproduces P1: agent ignores rustcodegraph — but read-once-and-edit is *efficient* here.)
  - **hook (deny-redirect):** 0 *successful* Reads + 1 file-view call (parity worked, edit compiled), but **8–9 tool calls, 9–10 turns, 200–239s**, and the agent **fought the deny** — `ToolSearch` to find the tool, reflexive re-Read (denied), then **`Bash python3` to read the file around the block.**
  - Verdict: a blanket Read-deny **regresses the target metrics (~2× tool calls, more turns) on a simple edit** and the agent routes around it. Forcing is the wrong lever; making the tool genuinely better than Read is the right one.
- If routing is ever revisited: not a blanket hook. Either a narrow trigger (large
  files only / after-N-reads) **with a clean A/B on a Read-heavy multi-file task**
  (the hook's best case, untested), or just keep widening coverage + sufficiency.

---

**Symptom:** even with rustcodegraph attached + the new steering, the agent reflexively `Read`s/`grep`s mid-implementation, and never reaches for the file-view. Descriptions can't fix this (low-salience wall).

### Ideas, ranked by expected leverage

1. **PreToolUse(Read/Grep) hook that redirects to rustcodegraph** — *highest leverage; the only channel that actually changes behavior.*
   - Claude Code **hooks** can intercept a tool call and inject context or block it — unlike descriptions, this is *not* low-salience. We already have `scripts/agent-eval/block-read-hook.sh` + `hook-settings.json` (used to force Read=0 in evals).
   - Ship a **recommended (opt-in) hook**: on `Read` (or `Grep`) of a path that's *indexed*, inject "this file is indexed — `rustcodegraph_node {file}` returns it + its blast radius for fewer tokens; treat its output as already-Read." Soft nudge (don't hard-block, or it'll frustrate users on configs/docs rustcodegraph doesn't index).
   - The installer (`src/installer/targets/claude.rs`) could offer to add this hook (opt-in, like the auto-allow permissions).
   - **Validate** with `ab-new-vs-baseline.sh` (Read count, with vs without the hook). This is the experiment most likely to move the needle.
   - Open Qs: how to know a path is indexed from inside a hook (query `rustcodegraph files`/`status`, or a fast local check against `.rustcodegraph`); avoiding noise on non-indexed files; per-language false positives.

2. **Sufficiency: make the file-view the obvious Read replacement so the agent *wants* it.**
   - The A/B showed the agent never passed a `file` to `rustcodegraph_node`. Why? It doesn't think "Read this file" → "rustcodegraph_node file=X". Investigate: is the file-view's value (symbols + dependents + bodies) actually *better than Read* for the agent's next step (an `Edit`)? It returns bodies — but does it return enough surrounding context to `Edit` confidently? If not, the agent Reads anyway.
   - Consider: when the agent *does* Read an indexed file, is there a way to make rustcodegraph's prior `explore`/`node` output have *already* given it what it needed? (i.e. fix the upstream sufficiency, not the Read itself.)

3. **Coverage — the durable lever.** Every flow that connects statically is one the agent doesn't Read to reconstruct. Keep closing dynamic-dispatch gaps (`src/resolution/`). Less about "stop Reading," more about "never need to."

4. **Naming / affordance experiments (low confidence, cheap).** The file-view is buried inside `rustcodegraph_node`. A dedicated, obviously-named affordance might get picked more — *but* "new tools fare worse," so this likely loses. If tried, A/B it; don't assume.

**Recommendation:** prototype **idea 1 (the Read-redirect hook)** and A/B it. It's the one lever with a real chance of moving behavior. Everything else is incremental.

---

## P2 — Agent runs without rustcodegraph because the server is `pending` at startup

**Symptom:** `serve --mcp` isn't ready when the agent's first turn fires (the host marks the MCP server `status:"pending"` / 0 tools), so the agent starts Read/grep and never uses rustcodegraph. We saw this hard in nested evals (~2-3s startup vs the agent's turn-1); **real users hit a milder version** — the first query of a session may not have rustcodegraph.

### Root cause
Before the Rust runtime, `serve --mcp` did a Node/V8 flag re-exec and then spawned/bound a detached daemon before tools were usable. The Rust runtime retired that Node/WASM re-exec; the remaining startup risk is daemon spawn/bind latency under load. Pre-warming a daemon still removes that bind latency in evals, but a real user should not need to do it manually.

### Ideas, ranked

1. **RUSTCODEGRAPH-SIDE — expose the static tool list INSTANTLY, decoupled from the daemon. *Biggest shippable win; helps every user.***
   - Hypothesis: the host marks rustcodegraph `pending` because `tools/list` (tool exposure) waits on the daemon connect. The local handshake path lives in `src/mcp/proxy.rs`. **Investigate: does `serve --mcp` answer `tools/list` locally and instantly, or does it forward it to the still-connecting daemon?** If the latter, decouple it: advertise the static tools the moment the client asks, mark connected, and resolve the daemon in the background for actual tool calls.
   - Verify with: `printf '<initialize>\n<initialized>\n<tools/list>\n' | target/release/rustcodegraph serve --mcp --path <repo>` and time the `tools/list` response, daemon-mode vs in-process. In-process answered in ~165ms; daemon-mode is the suspect.
   - If this lands, `pending`-at-startup largely disappears without any host change.

2. **DONE BY RUST RETIREMENT — no Node/WASM re-exec on the MCP serve path.** Keep startup work focused on daemon binding and static `tools/list` exposure.

3. **RUSTCODEGRAPH-SIDE — a SessionStart hook that pre-warms the daemon.** Ship an opt-in Claude Code `SessionStart` hook (installer-added) that spawns/warms the daemon for the project at session start, so it's bound before the first query. Mitigation if (1) is hard.

4. **HOST-SIDE — "wait/retry on pending" — this is what you asked about, but it's a Claude Code (MCP client) behavior, not rustcodegraph's to fix.** rustcodegraph can't make the agent retry. Options: (a) raise it with Anthropic as an MCP-client improvement (don't let the agent's first turn proceed until configured MCP servers finish connecting, or retry `pending` servers); (b) note `MCP_TIMEOUT` exists but did **not** help here, because the problem is *tool exposure timing*, not a connection timeout. Frame this as a request, and lean on (1)–(3) for what we control.

**Recommendation:** chase **idea 1** (decouple `tools/list` from the daemon). It's the fix that makes rustcodegraph "connected" instantly for everyone. Ship **idea 3** (pre-warm SessionStart hook) as a cheap mitigation in parallel. File the host-side request (4) but don't depend on it.

---

## Key files / pointers

- **Steering / tools:** `src/mcp/server_instructions.rs` (the `initialize` instructions — single source of truth), `src/mcp/tools.rs` (tool descriptions + handlers and `get_static_tools`).
- **Startup / daemon / proxy:** `src/mcp/proxy.rs`, `src/mcp/index.rs`, `src/mcp/daemon.rs`, and `src/mcp/ppid_watchdog.rs`.
- **Hooks (existing):** `scripts/agent-eval/block-read-hook.sh`, `scripts/agent-eval/hook-settings.json` (the eval's force-Read-0 hook — basis for the P1 redirect hook).
- **Installer (where to add a recommended hook):** `src/installer/targets/claude.rs`.
- **Eval harness:** `scripts/agent-eval/ab-new-vs-baseline.sh` (new-vs-baseline, pre-warm baked in), `run-all.sh` (with-vs-without), `rustcodegraph agent-eval parse-run` (tool-by-type counts; `rustcodegraph tools exposed: 0` + 0 rustcodegraph calls = ran without).
- **Doctrine:** `CLAUDE.md` → "Retrieval performance & dynamic-dispatch coverage" + the agent-eval note under "Validation methodology".

## How to validate anything here
- **P1 (Read displacement):** `bash scripts/agent-eval/ab-new-vs-baseline.sh <indexed-repo> "<implementation task>" [baseline-ref]` — compare `Read` vs `mcp__rustcodegraph__*` counts. ≥2 runs/arm (n=1 is noisy). Run non-nested for cleanest results. Use a *genuinely new* feature task (verify it doesn't already exist — the first A/B attempt wasted a run on an already-implemented `--quiet`).
- **P2 (startup):** time `tools/list` from `serve --mcp` (above); and count cold-start runs where `init` shows `connected` + tools > 0. Don't trust a single `pending` init snapshot — confirm by whether the agent actually called rustcodegraph.

## Constraints / gotchas to remember
- Descriptions/instructions are low-salience — **A/B every behavioral claim**, don't ship wording on faith.
- New tools < extending existing ones.
- The host's `init` snapshot can say `pending` even when the server then connects — judge by actual usage.
- Don't run evals nested for "clean" numbers unless pre-warmed; even then, a real terminal is better.

## Suggested start order for the fresh session
1. **P2 idea 1** — verify whether `serve --mcp` answers `tools/list` locally/instantly; if not, decouple it from the daemon. (Highest-value, shippable, helps all users, no behavioral guesswork.)
2. **P1 idea 1** — prototype the PreToolUse(Read) redirect hook; A/B it. (Highest-value behavioral lever.)
3. Ship the P2 SessionStart pre-warm hook as a mitigation; file the host-side wait/retry request.
