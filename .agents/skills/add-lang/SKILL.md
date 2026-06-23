---
name: add-lang
description: Add tree-sitter language support to codegraph end-to-end - wire the native Rust grammar + extractor, write tests, then benchmark extraction quality and retrieval value on 3 popular real-world repos. Use when the user runs /add-lang <language> or asks to add/support a new language (e.g. Lua, Elixir, Zig, OCaml) in codegraph.
---

# Add a language to CodeGraph

Wire a new native tree-sitter language into the Rust extraction pipeline, prove
it extracts real symbols on popular repos, and prove it helps an agent more than
no CodeGraph. Runs autonomously: pick repos, benchmark, update docs, then
report. Never commit, push, publish, or tag.

The argument is the lowercase language token used in `Language`, such as
`lua`, `elixir`, or `zig`. If none was given, ask which language. Use a stable
single-token form everywhere (`csharp`, not `c#`).

## Prerequisites

- Run from the codegraph repo root.
- `git`, `gh`, Rust stable, and a logged-in Codex CLI are available.
- The benchmark uses the local dev build. Build and link it once before the
  benchmark loop: `npm run build && ./scripts/local-install.sh`.

## Workflow

Copy this checklist and work through it in order:

```text
- [ ] 1. Resolve language; bail early if already supported
- [ ] 2. Add/select a native tree-sitter grammar crate
- [ ] 3. Health-check grammar and inspect AST with Rust helpers
- [ ] 4. Wire the language in Rust
- [ ] 5. Build + verify-extraction loop until PASS
- [ ] 6. Add extraction tests; make them green
- [ ] 7. Auto-pick 3 popular repos by size tier; add to corpus.json
- [ ] 8. Benchmark all 3: extraction + with/without A/B
- [ ] 9. Update README + CHANGELOG
- [ ] 10. Report; do NOT commit
```

## Step 1 - Resolve + Short-Circuit

Check whether the language is already wired:

- `src/types.rs` - `Language` enum and `LANGUAGES`
- `src/extraction/grammars.rs` - `NATIVE_GRAMMAR_REGISTRY`, `EXTENSION_MAP`,
  `get_language_display_name`, and `language_key`
- `src/web_tree_sitter.rs` - `native_language`
- `src/extraction/languages/index.rs` - extractor module exports and
  `extractor_for`

If the language is already supported, skip implementation and go straight to
benchmarking to validate retrieval value.

## Step 2 - Add Or Select The Grammar

Use a native Rust tree-sitter crate, not a `.wasm` grammar. Add the dependency
to `Cargo.toml` and wire it in `src/web_tree_sitter.rs::native_language` before
running the add-lang helpers. If no maintained Rust grammar crate exists, stop
and report the blocker rather than shipping a half-wired language.

For languages whose public token differs from the crate symbol, keep the public
token stable in CodeGraph and do the mapping in `native_language`.

## Step 3 - Health-Check And Inspect AST

Create a syntactically valid sample covering functions, classes/structs,
imports, enums, variables, and calls. Then use the Rust-owned helpers:

```bash
cargo run --bin codegraph -- add-lang check-grammar <lang> path/to/sample.ext
cargo run --bin codegraph -- add-lang dump-ast <lang> path/to/sample.ext --depth=6
```

`check-grammar` loads the native grammar through CodeGraph's Rust parser facade
and parses the sample repeatedly. `dump-ast` prints a bounded tree view with
field names plus a named-node frequency table. Use the frequency table to decide
which node types map to functions, classes, imports, calls, variables, and
type aliases.

## Step 4 - Wire The Language

Make the wiring edits in the existing Rust style:

1. `Cargo.toml` - add the `tree-sitter-<lang>` dependency.
2. `src/types.rs` - add a `Language` enum variant and include it in
   `LANGUAGES`.
3. `src/extraction/grammars.rs` - add the token to
   `NATIVE_GRAMMAR_REGISTRY`, extensions to `EXTENSION_MAP`, a display name,
   and a `language_key` match arm.
4. `src/web_tree_sitter.rs` - map the `Language` variant to the native grammar
   crate in `native_language`; update `Language::load` token detection if
   needed.
5. `src/extraction/languages/<lang>.rs` - add a `LanguageExtractor`
   implementation modeled on the closest existing language.
6. `src/extraction/languages/index.rs` and `src/lib.rs` - expose the new
   extractor module and add the token to `extractor_for`.

Sometimes `src/extraction/tree_sitter.rs` needs a small language-specific
branch when the grammar nests declared names in a way the generic extractor
cannot see. Keep that branch narrow and covered by tests.

## Step 5 - Build + Verify Loop

Build the local Rust binary, index a sample repo, then verify extraction:

```bash
npm run build
( cd <sample-repo> && codegraph init -i )
codegraph add-lang verify-extraction <sample-repo> <lang>
```

The verification fails if the language was not detected or if indexing produced
only structural file/import/export nodes. On failure, re-run `dump-ast`, fix the
extractor mappings, rebuild, re-index, and re-verify until it passes.

## Step 6 - Tests

Add coverage in `tests/extraction_test.rs`:

- language detection for the extension
- extraction of representative functions/classes/imports/calls from inline
  source
- any grammar-specific variable, method, receiver, or import behavior you had
  to special-case

Run:

```bash
cargo test --test extraction_test -- --test-threads=1
```

## Step 7 - Auto-Pick 3 Repos + Corpus

Pick without asking. Find candidates, then curate three that are genuinely
language-dominant, one per size tier:

```bash
gh search repos --language=<lang> --sort=stars --limit 40 \
  --json fullName,stargazerCount,description
```

Use one small repo (<~150 files), one medium repo (~150-1500 files), and one
large repo (>~1500 files). Write one cross-file architecture question per repo
and add the set to `.agents/skills/agent-eval/corpus.json` if that corpus is
being used for the evaluation.

## Step 8 - Benchmark All 3

Make the dev build available on PATH once, then loop:

```bash
npm run build && ./scripts/local-install.sh
scripts/add-lang/bench.sh <lang> <name> <url> "<question>" headless
```

`bench.sh` clones or reuses the repo, wipes and indexes `.codegraph`, runs
`codegraph add-lang verify-extraction`, then runs retrieval A/B through
`scripts/agent-eval/run-all.sh`. Report tool calls, file Reads, Grep/Bash,
CodeGraph tool calls, duration, and cost for both arms.

## Step 9 - Docs + CHANGELOG

- `README.md`: add the language to the feature bullet and supported languages
  table.
- `CHANGELOG.md`: add a friendly `## [Unreleased]` entry under
  `### New Features` explaining the language support from a user's perspective.

## Step 10 - Report

Summarize for review:

- files changed
- extraction result per repo: files, nodes, edges, verification result
- A/B result per repo: with vs without CodeGraph
- gaps or follow-ups, such as unmapped node types or missing framework edges

Leave the changes uncommitted. Releases go through the GitHub Actions Release
workflow.

## Notes

- The A/B spawns real paid Codex runs. Keep model and effort aligned with the
  repository's agent-eval scripts unless the maintainer explicitly says
  otherwise.
- Do not use `.wasm` grammar files for new runtime support. The Rust runtime
  uses native tree-sitter crates.
- An index must be served by the same binary that built it. Step 8 builds and
  links the dev binary first, so this holds.
