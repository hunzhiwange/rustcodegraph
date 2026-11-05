# Third-party grammars

Third-party parser sources belong to upstream repositories, not this repository's
Git blobs. The seven `vendor/tree-sitter-*` entries are Git submodules; their
committed gitlinks pin exact revisions. Other languages use Cargo dependencies.

## Build from a checkout

```sh
git clone https://github.com/hunzhiwange/rustcodegraph.git
cd rustcodegraph
# Also run this after pulling a commit that changes submodule revisions:
git submodule update --init
npm run prepare:grammars
cargo build --release
```

Preparation requires Git, Node.js 18+ and npm. The first run obtains
`tree-sitter-cli@0.25.10` via npx; subsequent runs reuse npm's cache and skip
unchanged prepared grammars. Normal installed binaries need none of these tools.
`node scripts/prepare-grammars.mjs --check` checks preparation without generation
or downloads. Cargo itself never fetches or patches grammar sources.

The native build disables compiler warnings only for these third-party parser and
scanner compilation units. Compilation errors and Rust diagnostics remain enabled;
upstream warning cleanup does not require local source patches.

Only direct submodules are needed. Avoid `--recursive`/`--recurse-submodules`:
ArkTS has an unrelated nested test-fixture repository that is not a build input.
CI uses `actions/checkout` with `submodules: true` for this reason.

## Pinned sources

| Submodule | Upstream | Revision | Build path |
|---|---|---|---|
| Terraform | [tree-sitter-grammars/tree-sitter-hcl](https://github.com/tree-sitter-grammars/tree-sitter-hcl) | `fad991865fee927dd1de5e172fb3f08ac674d914` | `dialects/terraform/src` |
| Solidity | [JoranHonig/tree-sitter-solidity](https://github.com/JoranHonig/tree-sitter-solidity) | `b239a95f94cfcc6e7b3e961bc73a28d55e214f02` | `src` |
| Erlang | [WhatsApp/tree-sitter-erlang](https://github.com/WhatsApp/tree-sitter-erlang) | `836aa2b6c3af2c7cef3f84049b0ed6d44485a870` | `src` |
| VB.NET | [govindbanura/tree-sitter-vbnet](https://github.com/govindbanura/tree-sitter-vbnet) | `538b7087bf80e86004531b392fe1186379c0a2b5` | prepared output |
| COBOL | [yutaro-sakamoto/tree-sitter-cobol](https://github.com/yutaro-sakamoto/tree-sitter-cobol) | `e99dbdc3d800d5fa2796476efd60af91f6b43d93` | prepared output |
| CFML | [cfmleditor/tree-sitter-cfml](https://github.com/cfmleditor/tree-sitter-cfml) | `24051c091c9c1a69b36914533aee7aa3286d5526` | `cfml/src`, `cfscript/src` |
| ArkTS | [harmony-contrib/tree-sitter-arkts](https://github.com/harmony-contrib/tree-sitter-arkts) | `56f7fc288715befe92c734d603f9e6bc3c65d2a8` | `src` |

VB.NET and COBOL use CodeGraph patches originating from commit
`15483713763f344ff30fb7df92bcf091c7bd9758`. The COBOL patch additionally
replaces variable-length scanner arrays with compile-time sizes derived from its
keyword table so it compiles with Windows MSVC. The current SHA-256 values are:

- VB.NET: `89124753d87a89a54f3f915bfdbec61cff81d3c6106fbd243633440233fbb8fa`
- COBOL: `331b7a1114c130cc053563e57553c3728ebf833028ad359729eb56da8d8d9a7f`

The patches and generation configurations are kept here; all prepared C sources live
only in ignored `.grammar-build/`. Unpatched grammars are staged there unchanged
because Cargo excludes nested packages inside submodules from source archives.
Submodules remain unmodified. Upstream licenses
remain in each submodule, and the CodeGraph MIT notice is retained beside patches
and copied into prepared output. CFScript uses the original upstream scanner;
the previous vendored copy differed only in trailing whitespace.

## Adding or updating a language

1. Prefer a compatible published Cargo crate; use a pinned Cargo Git dependency
   when suitable Rust bindings exist only upstream.
2. Otherwise add a Git submodule at `vendor/tree-sitter-<language>`, pin a tested
   commit, and compile its existing sources. Do not copy third-party C/header,
   parser tables, generated JSON, or grammar source into the parent repository.
3. If patches or generation are required, retain a small reviewable patch and
   pinned generator configuration; prepare in ignored output, never modify the
   submodule or commit generated files. Extend the preparation script as needed.
4. Update this provenance table, build wiring, source-package includes, CI,
   licenses, and focused language regression coverage. Verify all submodules stay
   clean and `git ls-files --stage vendor` contains only mode `160000` entries.
   CI enforces this with `node scripts/check-grammar-dependencies.mjs`.
5. To update a submodule, fetch and checkout the chosen commit inside it, prepare
   grammars again, run relevant tests, and commit the parent gitlink change.
   Do not use floating branches or `git submodule update --remote` in CI.

A plain GitHub source archive does not contain submodule contents. Build from a
checkout with initialized direct submodules or a prepared Cargo source package. The Cargo package includes
the required upstream C sources and prepared output so consumers do not need Git,
Node, or generation during compilation. It is intentionally larger than the
parent repository's tracked source snapshot. Existing Git history is unchanged.

Currently, full `cargo package`/crates.io publication is blocked by the existing
`tree-sitter-nix` Git dependency lacking a registry version requirement. This
migration checks the package file list and builds those prepared inputs separately;
it does not change Nix dependency publication policy or publish a package.

Historical native feasibility evidence remains in `docs/design/upstream-grammars/`;
its old vendoring instructions are superseded by this dependency workflow.
