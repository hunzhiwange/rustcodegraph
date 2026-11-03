# Nix native grammar feasibility

Scope: grammar feasibility only; this does not enable Nix indexing.

## Provenance

Checked on 2026-11-03 against CodeGraph commit
`15483713763f344ff30fb7df92bcf091c7bd9758`, whose
`src/extraction/grammars.ts` records nix-community/tree-sitter-nix
`3d0173d903e630b6e14d17f1cf79488791379ded`, MIT, regenerated with
 tree-sitter-cli 0.25.10 for an ABI 15 WASM. Its checked-in parser is ABI 13.
The comment establishes declared provenance, not byte-for-byte reproducibility
of the WASM build. WASM strings alone do not establish a source commit.

Official source: <https://github.com/nix-community/tree-sitter-nix>.
The native crate is `tree-sitter-nix` 0.3.0; pin the full git revision rather
than the crate version alone. Its Rust binding exports `LANGUAGE` through
`tree-sitter-language`; `bindings/rust/build.rs` compiles `src/parser.c` and
`src/scanner.c` with `cc`. Preserve the MIT copyright/license on redistribution.
No WASM runtime or Node dependency is needed.

## AST contract

| Construct | Nodes and fields |
| --- | --- |
| `let f = x: x; in ...` | `let_expression`, `binding_set`, `binding` (`attrpath`, `expression`), `function_expression` (`universal`, `body`) |
| `f 1` | `apply_expression` (`function`, `argument`), `variable_expression` (`name`) |
| `import ./foo.nix` | ordinary `apply_expression`, callee identifier `import`, argument `path_expression` / `path_fragment` |
| `{ inherit f; }` | `attrset_expression`, `inherit`, `inherited_attrs` |

Imports are ordinary applications; the later extractor must distinguish static
project paths from dynamic imports rather than infer file edges from every call.
Curried functions nest function expressions. Broken input must retain error or
missing-node information; UTF-8 ranges are byte offsets.

## Reproduction

Create an isolated `[workspace]` probe with tree-sitter `=0.26.10` and a path
checkout of the grammar at the revision above. The local probe lives under
`.codex-tmp/native-grammar-probes/nix` and retains its lockfile and raw logs.
Before Cargo, dynamically acquire a repository shared slot and set absolute
`CARGO_TARGET_DIR`; also set `CARGO_BUILD_BUILD_DIR` to `$CARGO_TARGET_DIR/build`
because this repository configures a separate `build-dir=target`.
Run `cargo fmt --all`, then
`cargo test --manifest-path .codex-tmp/native-grammar-probes/nix/Cargo.toml -- --nocapture`.
The three real parser tests cover declarations, calls/imports, and damaged plus
multibyte input. This experiment deliberately adds no production dependency.

## Verified result

Exact-source native run: **3 passed, 0 failed, 0 ignored**, exit 0, ABI **13**.
The ABI 15 observation from the earlier latest-HEAD run belongs to
`e49c2f317e896eb08967717e758a46d6eea038b2`, not the pinned source revision.
Both are accepted by tree-sitter 0.26.10; the selected reproduction uses the
upstream-declared source revision and its unmodified native parser.
No regeneration is necessary for this native feasibility result.

Actual slot: **1**, `CARGO_TARGET_DIR` was the absolute repository path ending
in `target/rust-slot-1`; `CARGO_BUILD_BUILD_DIR` ended in
`target/rust-slot-1/build`. Cargo JSON identified the test executable under
`target/rust-slot-1/build/debug/deps/`. The slot lock was released after exit.
Raw evidence: local `exact-source.log` (JSON artifacts plus test output),
`retry.log` (latest-source comparison), `test.log` (initial experiment).
`cargo fmt --all` and `git diff --check` exited 0.

This feasibility task keeps the three grammar tests in the isolated probe,
rather than adding a root integration target requiring an uninstalled grammar
or changing production Cargo dependencies. There is no ignored/empty root
`upstream_062_test` placeholder. For reproducibility, the complete probe source
is below (dependencies: tree-sitter =0.26.10 and pinned tree-sitter-nix path).

```rust
#[cfg(test)]
mod tests {
    fn parse(s: &str) -> tree_sitter::Tree {
        let language = tree_sitter_nix::LANGUAGE.into();
        let mut p = tree_sitter::Parser::new();
        p.set_language(&language).unwrap();
        println!("ABI {}", language.abi_version());
        let t = p.parse(s, None).unwrap();
        println!("{}", t.root_node().to_sexp());
        t
    }
    #[test]
    fn declaration() {
        let t = parse("let f = x: x; in { inherit f; }");
        assert!(!t.root_node().has_error());
        assert!(t.root_node().to_sexp().contains("function_expression"));
    }
    #[test]
    fn call_and_import() {
        let t = parse("let f = import ./foo.nix; in f 1");
        assert!(!t.root_node().has_error());
        let ast = t.root_node().to_sexp();
        assert!(ast.contains("apply_expression"));
        assert!(ast.contains("path_expression"));
    }
    #[test]
    fn damaged_and_multibyte() {
        assert!(parse("let x = ; in").root_node().has_error());
        let s = "{ message = \"你好🌍\"; }";
        let t = parse(s);
        assert!(!t.root_node().has_error());
        assert_eq!(t.root_node().end_byte(), s.len());
    }
}
```
