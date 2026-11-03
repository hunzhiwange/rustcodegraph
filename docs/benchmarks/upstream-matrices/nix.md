# Frozen Nix repository matrix

This manifest freezes nine **static, bounded** Nix flow questions: three in each
of three real upstream repositories. It prepares the individual acceptance runs;
it does not claim Agent A/B performance or complete Nix evaluation coverage.

The JSON array follows `scripts/agent-eval/run-matrix-case.py`'s contract.
Additional fields freeze license hashes, indexed file counts, source files, and
the JavaScript control repository's URL and commit. `nix-evidence.json` records
ordered Flow lines and exact database nodes/edges, rather than accepting a
successful process exit as proof of connectivity.

## Reproduce

All `local_path` and `control_repo` directories are disposable acceptance copies
under `/tmp/rcg-nix-matrix-069`. Recreate a missing copy from its `repo_url` and
full `commit` (fetch that commit explicitly and use a detached checkout). Never
index an unrelated user checkout. Obtain each distinct repository once:

```python
import json, pathlib, subprocess
cases = json.loads(pathlib.Path('docs/benchmarks/upstream-matrices/nix.json').read_text())
repos = {(c['repo_url'], c['commit'], c['local_path']) for c in cases}
repos |= {(c['control_repo_url'], c['control_commit'], c['control_repo']) for c in cases}
for url, commit, path in sorted(repos):
    if pathlib.Path(path).exists():
        actual = subprocess.check_output(['git', '-C', path, 'rev-parse', 'HEAD'], text=True).strip()
        assert actual == commit, (path, actual, commit)
        continue
    pathlib.Path(path).parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(['git', 'init', path], check=True)
    subprocess.run(['git', '-C', path, 'remote', 'add', 'origin', url], check=True)
    subprocess.run(['git', '-C', path, 'fetch', '--depth', '1', 'origin', commit], check=True)
    subprocess.run(['git', '-C', path, 'checkout', '--detach', 'FETCH_HEAD'], check=True)
```

Use a binary built from implementation commit
`51057670b77b35cb25a9cef587deff82e3828315`, default features. The original
source reference is the read-only TypeScript upstream
`15483713763f344ff30fb7df92bcf091c7bd9758`. `baseline_ref` is the immutable
commit `7c2afed5eeac4560eb76aab8bfa572e290ac9484`, immediately before native
Nix language detection/grammar wiring; it is a RustCodeGraph commit, not a
sample repository commit. It deliberately includes the matrix runner. The
control query follows Express application creation, initialization, and default
configuration, using existing JavaScript capability.

Index the acceptance copies with `rustcodegraph init -i <local_path>`; compare
`select count(*) from files` with `indexed_file_count`. Small is fewer than 500
indexed files, medium is 500–4,999, and large is at least 5,000. The large sample uses the immutable Nixpkgs 17.03
release commit rather than the current tree, to keep repeated acceptance runs
practical while retaining a complete, unmodified real repository. These categories
use the indexed file count, not all tracked files or only `.nix` files.

For every case, run:

```text
rustcodegraph agent-eval probe-explore <local_path> <symbols...>
```

The acceptance binary uses default features with release optimization level 1
for the root package, built in Cargo slot 1 with both `CARGO_TARGET_DIR` and
`build.build-dir` confined to that slot.

Check the ordered `expected_path` in the Flow section, then verify that adjacent
nodes have `calls` edges and source locations agree with the fixed checkout.
The evidence includes source and license SHA-256 hashes for this audit.
The original binary path, commands, interrupted attempts, and raw logs are in
`/tmp/rcg-nix-matrix-069/`; the committed evidence is sufficient to identify the
fixed inputs even after this temporary directory is removed.

```bash
python3 -m json.tool docs/benchmarks/upstream-matrices/nix.json
```

Individual later acceptance tasks can invoke the single-case runner with a
fresh absolute `--out` directory and `--case small-1` through `large-3`. Those
runs require sonnet/high, actual MCP use, two runs per arm, daemon prewarming,
and separate baseline/control regression review. None of those A/B results is
implied by freezing this manifest.

## Scope and rejected candidates

The initial flake-utils question `eachDefaultSystem → eachSystem → eachSystemOp`
did not produce a connected Flow. Partial applications remain outside these
cases. The current Nixpkgs and a debug-build Home Manager index were interrupted
while preparing samples; only completed release-build indexes supply counts. The recursive `op`/`recurse` candidate also exposed an unrelated same-name
`op` caller; it was replaced with the direct `loadOverlay → maybeImport` path.
The selected questions stop before dynamic import targets, builtin evaluation,
and runtime module composition. All selected source files are covered by the
repositories' MIT license notices; individual packaged software licenses in
Nixpkgs are not being characterized here.

## Lookup preparation and regression limits

Preparing the full repositories exposed repeated whole-project candidate scans.
The accompanying resolver change indexes qualified names, member names, and
normalized reference names once per pass. Candidate buckets retain input order;
the existing eligibility checks, score, and equal-score selection remain intact.
It does not bridge the rejected partial-application flows.

The final focused run includes Nix import, incremental-sync, synchronization,
resolver, and candidate-selection checks. The resolver suite also reports its
existing ignored tests explicitly. The framework integration suite has two
Java field-injection failures reproduced on the unmodified implementation;
these remain pending regression work outside the Nix matrix. The same two
failures occur with the lookup indexes, with no additional framework failures.

Express's complete node set and `calls` edges are compared with the original
index. A few `instantiates` edges choose different duplicate source positions
across unmodified baseline rebuilds as well, so these are recorded separately
rather than described as identical. Nix graph comparisons normalize metadata
JSON key order. Flake-utils matches all selected node fields and full edges;
Home Manager retains identical nodes, call edges, and full source/target/kind
topology. Duplicate source-position records fluctuate in Home Manager on both
unmodified baseline rebuilds and optimized builds; its extra eight edge rows
were also reproduced by the unmodified baseline. Raw before/repeat/after
comparisons are retained with the experiment logs.
