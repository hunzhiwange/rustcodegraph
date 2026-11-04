# VB.NET native grammar feasibility

Confirmed 2026-11-04, batch 134, no dependencies. This prerequisite does not
add VB.NET extraction or claim complete language coverage.

## Provenance and license

Read-only CodeGraph HEAD is `15483713763f344ff30fb7df92bcf091c7bd9758`.
Its `docs/grammars/tree-sitter-vbnet.md` pins
[govindbanura/tree-sitter-vbnet](https://github.com/govindbanura/tree-sitter-vbnet/tree/538b7087bf80e86004531b392fe1186379c0a2b5)
at `538b7087bf80e86004531b392fe1186379c0a2b5`, plus the CodeGraph
`docs/grammars/tree-sitter-vbnet.patch` from that fixed HEAD.
Patch SHA-256: `89124753d87a89a54f3f915bfdbec61cff81d3c6106fbd243633440233fbb8fa`.
The checkout and patch applied successfully locally; no guessed crates.io dependency.
The pinned [LICENSE](https://github.com/govindbanura/tree-sitter-vbnet/blob/538b7087bf80e86004531b392fe1186379c0a2b5/LICENSE)
is MIT, copyright 2025 Govind Banura. Retain its copyright and permission
notice when vendoring. CodeGraph's root LICENSE is MIT as well; retain that
notice for its modified grammar/scanner. No grammar source is committed here.

## Native build

The grammar checkout has no generated parser. Apply the pinned patch (including
its new `src/scanner.c`), create tree-sitter.json below, and run
`npx --yes --package tree-sitter-cli@0.25.10 tree-sitter generate`.
Generator 0.25+ is required for Unicode regex `/u`; do not use 0.24.
Generation exited 0 with unnecessary-conflict warnings only.
Compile generated `src/parser.c` AND patched `src/scanner.c`, C11, include `src`.
The entry is `tree_sitter_vbnet`; scanner exports use that same prefix.
`LanguageFn::from_raw(...).into()` loads successfully in tree-sitter 0.26.10;
observed/asserted ABI is **15**. This is a viable native wrapper approach without
Node/WASM at runtime. Root Cargo.toml and Cargo.lock remain unchanged.

## Observed AST and limits

| Syntax | Native observation |
| --- | --- |
| Namespace | `namespace_declaration name: simple_name(identifier)` |
| Class/module | `class_declaration` / `module_declaration`, `name: identifier`, `body: method_declaration` |
| Function/Sub | `method_declaration name`, `parameters: parameter_list`; Function has `as_clause declared_type`; statements are direct children (no method body wrapper) |
| Parameters | `parameter name: identifier`, `as_clause declared_type: predefined_type` |
| Imports alias | `imports_statement alias: identifier namespace: qualified_name`, qualifier/name are simple_name wrappers |
| Empty call | `invocation_expression function: identifier arguments: argument_list` |
| Member call with argument | `array_access_expression array: member_access_expression object/member`, `indices: integer_literal`; call vs indexing needs semantic disambiguation |
| Local variable | `declaration_statement` → `variable_declarator name`, `initializer: string_literal` |

Indented declaration and call/import samples parse without errors. Chinese
identifiers and Chinese/emoji strings parse without errors; every node's byte
range slices the UTF-8 source safely and root end byte equals input length.
Damaged input returns an ERROR region. Missing final newline returns a missing
terminator; future integration must preserve the upstream trailing-newline
normalization, without shifting existing offsets.

Initial valid declaration sample with a **column-zero Return** failed with a
MISSING `:` before return_statement (2 passed, 1 failed; exit 101). Merely
indenting Return gives a clean parse. This is a confirmed grammar limitation,
not a Rust ABI failure; do not promise arbitrary valid VB parses cleanly.
The local initial-failure.log preserves it. Other known upstream limitations
include column-zero labels and opaque XML literal tokens; scanner behavior and
full extraction/resolution are not established by these three samples.

## Checks and reproduction

Local ignored crate: `.codex-tmp/native-grammar-probes/vbnet/`, name
`native_grammar_probe_vbnet`, default features, isolated `[workspace]`.
Final **3 passed, 0 failed, 0 ignored**, 0 doc tests; exit **0**.
Both stages atomically acquired shared **slot 1**:
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1`;
`CARGO_BUILD_BUILD_DIR=$CARGO_TARGET_DIR/build` kept intermediates in the slot.
Owner locks were removed after process exit; caches retained.

Commands: `cargo fmt --all`;
`cargo fmt --manifest-path .codex-tmp/native-grammar-probes/vbnet/Cargo.toml --all`;
`cargo test --manifest-path .codex-tmp/native-grammar-probes/vbnet/Cargo.toml -- --nocapture`;
`git diff --check`. Final commands exit 0. Raw generator/test/failure logs and
Cargo.lock remain in the ignored local crate. No production behavior changed,
so production TDD is inapplicable. Actual native tests live there, consistent
with earlier feasibility reports; no empty/ignored upstream_134_test.rs shell
is added. Their complete sources are preserved below for reproduction.
Cancelled real-repository replay, Agent A/B, authentication and VM/Docker
checks were not run and are not claimed as passing.

Recreate: clone the grammar URL above into `grammar`, checkout the full commit,
apply the patch from fixed CodeGraph HEAD, generate with the pinned CLI, then
run Cargo under a dynamically acquired shared slot.

### grammar/tree-sitter.json

```json
{"grammars":[{"name":"vbnet","camelcase":"Vbnet","scope":"source.vbnet","path":".","file-types":["vb"]}],"metadata":{"version":"0.1.0","license":"MIT","description":"VB.NET grammar","links":{"repository":"https://github.com/govindbanura/tree-sitter-vbnet"}}}
```

### Cargo.toml

```toml
[package]
name = "native_grammar_probe_vbnet"
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
    cc::Build::new()
        .std("c11")
        .include("grammar/src")
        .file("grammar/src/parser.c")
        .file("grammar/src/scanner.c")
        .compile("tree-sitter-vbnet");
}

```

### src/lib.rs

```rust
#[cfg(test)]
mod tests {
    extern "C" {
        fn tree_sitter_vbnet() -> *const ();
    }
    fn parse(s: &str) -> tree_sitter::Tree {
        let lang: tree_sitter::Language =
            unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_vbnet) }.into();
        println!("ABI={}", lang.abi_version());
        assert_eq!(lang.abi_version(), 15);
        let mut p = tree_sitter::Parser::new();
        p.set_language(&lang).unwrap();
        let t = p.parse(s, None).unwrap();
        println!(
            "errors={} {}",
            t.root_node().has_error(),
            t.root_node().to_sexp()
        );
        t
    }
    fn clean(s: &str, kinds: &[&str]) {
        let t = parse(s);
        assert!(!t.root_node().has_error());
        for k in kinds {
            assert!(t.root_node().to_sexp().contains(k), "missing {k}");
        }
    }
    #[test]
    fn declarations() {
        clean("Namespace Demo\nPublic Class Sample\nPublic Function Run(x As Integer) As String\n  Return \"ok\"\nEnd Function\nEnd Class\nEnd Namespace\n", &["namespace_declaration","class_declaration","method_declaration","parameters:","as_clause","name:"]);
    }
    #[test]
    fn calls_and_imports() {
        clean("Imports AliasName = System.Text\nModule Demo\nSub Run()\nHelper()\nConsole.WriteLine(1)\nEnd Sub\nEnd Module\n", &["imports_statement","module_declaration","invocation_expression","array_access_expression"]);
    }
    #[test]
    fn damaged_and_multibyte() {
        assert!(parse("Class Broken\nSub (\n").root_node().has_error());
        let s = "Class 示例\nSub 运行()\nDim text = \"你好🌍\"\nEnd Sub\nEnd Class\n";
        let t = parse(s);
        assert!(!t.root_node().has_error());
        assert_eq!(t.root_node().end_byte(), s.len());
        fn check(n: tree_sitter::Node<'_>, s: &str) {
            assert!(s.get(n.byte_range()).is_some());
            for c in n.children(&mut n.walk()) {
                check(c, s);
            }
        }
        check(t.root_node(), s);
        assert!(parse(s.trim_end()).root_node().has_error());
    }
}

```
