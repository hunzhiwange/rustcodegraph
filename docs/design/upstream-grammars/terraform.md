# Terraform native grammar feasibility

Scope: native parser feasibility only; Terraform indexing is not enabled here.
Batch 80 has no dependencies. Checked 2026-11-04 against the read-only upstream
CodeGraph commit `15483713763f344ff30fb7df92bcf091c7bd9758`.

## Source and native entry

Upstream `src/extraction/grammars.ts` declares the Terraform WASM from
`@tree-sitter-grammars/tree-sitter-hcl` **1.2.0**, Apache-2.0.
The official npm version metadata records gitHead
`fad991865fee927dd1de5e172fb3f08ac674d914`; the probe checked out exactly that
revision of [tree-sitter-hcl](https://github.com/tree-sitter-grammars/tree-sitter-hcl/tree/fad991865fee927dd1de5e172fb3f08ac674d914).
This is declared source provenance; it does not establish byte identity between
an independently compiled native parser and upstream WASM.

`tree-sitter.json` declares separate HCL and Terraform grammars. The root Rust
crate `tree-sitter-hcl` 1.2.0 exports **only HCL**, so using its `LANGUAGE` would
silently test the wrong dialect. Terraform's generated C sources are
`dialects/terraform/src/parser.c` and `scanner.c`; compile both with `cc`, C11,
and that directory on the include path. The C entry is
`tree_sitter_terraform() -> *const ()`; adapt it through
`tree_sitter_language::LanguageFn::from_raw` into `tree_sitter::Language`.
No regeneration, WASM runtime, Node runtime, or production Cargo changes are
required. Future vendoring must retain the Apache-2.0 license and applicable
copyright/notice material. This experiment makes no Windows/Linux build claim.

## Observed AST contract

| Construct | Native AST |
| --- | --- |
| Resource declaration | `config_file → body → block`; header `identifier` followed by ordered `string_lit` labels, then nested `body` |
| Block attributes | `attribute → identifier + expression`; strings use `literal_value → string_lit → template_literal` |
| `upper(var.name)` | `function_call → identifier + function_arguments`; argument has `variable_expr` and sibling `get_attr` |
| Module `source = "./modules/app"` | ordinary `attribute` string, not a dedicated import node |

A later extractor must inspect block headers and attribute names, distinguish
literal local module paths from registry/git/dynamic sources, and preserve
traversal scope. Function calls are Terraform built-ins, not automatically
project function definitions. Broken input yields `ERROR`; multibyte string
input parses without errors and root end offsets equal UTF-8 byte length.

## Reproduction and evidence

Local untracked artifacts: `.codex-tmp/native-grammar-probes/terraform/`, with
pinned grammar checkout, Cargo.lock, npm metadata and raw `test.log`.
The probe is an isolated `[workspace]`, default features, crate
`native_grammar_probe_terraform`; dependencies are tree-sitter `=0.26.10`,
tree-sitter-language `0.1`, build dependency cc `1.2`.

Dynamically acquired shared **slot 1** using atomic mkdir. Actual
`CARGO_TARGET_DIR` was the absolute repository path ending in `target/rust-slot-1`;
`CARGO_BUILD_BUILD_DIR=$CARGO_TARGET_DIR/build` overrides this repository's
separate build-dir configuration. The test executable was under
`target/rust-slot-1/build/debug/deps/`. The lock was released after the stage;
caches remain.

Commands: `cargo fmt --all`,
`cargo fmt --manifest-path .codex-tmp/native-grammar-probes/terraform/Cargo.toml --all`,
then `cargo test --manifest-path .codex-tmp/native-grammar-probes/terraform/Cargo.toml -- --nocapture`.
All exited **0**. Result: **3 passed, 0 failed, 0 ignored**, ABI **15**.
The upstream scanner emits one unused-function compiler warning.
`git diff --check` also exited 0. Feasibility work has no production behavior
change and therefore no implementation red/green cycle. Tests live in the
isolated probe rather than an empty or ignored `tests/upstream_080_test.rs`
requiring a production grammar dependency.

External A/B, real repository replays and VM/Docker validation were not run,
as cancelled by the user; no external acceptance result is claimed.

Complete probe sources (place the pinned checkout in `grammar/`):

### Cargo.toml

```toml
[package]
name = "native_grammar_probe_terraform"
version = "0.1.0"
edition = "2021"
[workspace]
[dependencies]
tree-sitter = "=0.26.10"
tree-sitter-language = "0.1"
[build-dependencies]
cc = "1.2"
```

### build.rs

```rust
fn main() {
    let src = "grammar/dialects/terraform/src";
    cc::Build::new()
        .std("c11")
        .include(src)
        .file(format!("{src}/parser.c"))
        .file(format!("{src}/scanner.c"))
        .compile("tree-sitter-terraform");
}
```

### src/lib.rs

```rust
#[cfg(test)]
mod tests {
    extern "C" {
        fn tree_sitter_terraform() -> *const ();
    }
    fn parse(s: &str) -> tree_sitter::Tree {
        let language: tree_sitter::Language =
            unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_terraform) }.into();
        println!("ABI {}", language.abi_version());
        let mut p = tree_sitter::Parser::new();
        p.set_language(&language).unwrap();
        let t = p.parse(s, None).unwrap();
        println!("{}", t.root_node().to_sexp());
        t
    }
    #[test]
    fn declaration() {
        let t = parse("resource \"aws_s3_bucket\" \"demo\" {\n bucket = \"demo\"\n}\n");
        assert!(!t.root_node().has_error());
        for k in ["block", "identifier", "string_lit", "body", "attribute"] {
            assert!(t.root_node().to_sexp().contains(k), "{k}");
        }
    }
    #[test]
    fn calls_and_module_source() {
        let t =
            parse("module \"app\" {\n source = \"./modules/app\"\n name = upper(var.name)\n}\n");
        assert!(!t.root_node().has_error());
        for k in [
            "function_call",
            "variable_expr",
            "get_attr",
            "attribute",
            "string_lit",
        ] {
            assert!(t.root_node().to_sexp().contains(k), "{k}");
        }
    }
    #[test]
    fn damaged_and_multibyte() {
        assert!(parse("resource \"broken\" { x = ").root_node().has_error());
        let s = "output \"message\" {\n value = \"你好🌍\"\n}\n";
        let t = parse(s);
        assert!(!t.root_node().has_error());
        assert_eq!(t.root_node().end_byte(), s.len());
    }
}
```
