# ArkTS native grammar feasibility

Confirmed 2026-11-04, batch 188, no dependencies. The fixed read-only CodeGraph
checkout is `15483713763f344ff30fb7df92bcf091c7bd9758`. This report proves a
native binding option, not production extraction or full ArkTS coverage.

## Source and license

CodeGraph's `src/extraction/grammars.ts` identifies its vendored grammar as
`tree-sitter-arkts@0.2.0`, from
[harmony-contrib/tree-sitter-arkts](https://github.com/harmony-contrib/tree-sitter-arkts).
The [npm version metadata](https://registry.npmjs.org/tree-sitter-arkts/0.2.0)
gives gitHead `56f7fc288715befe92c734d603f9e6bc3c65d2a8` and tarball SHA-1
`7a562c46786a52187688cacd4054bc3dc9d08d90`.
Downloaded that exact npm tarball and the GitHub archive at that commit.
`cmp` of parser.c, scanner.c and common/scanner.h between both archives returned
0. The npm WASM and fixed CodeGraph vendored WASM have the same SHA-256:
`db0812971109457d22b3fe9dcdb1cd8e614fba2a338e220670bd254485753623`.
This ties the tested native source to the actual upstream package, without
claiming native/WASM execution equivalence for every input.

[LICENSE at the pinned commit](https://github.com/harmony-contrib/tree-sitter-arkts/blob/56f7fc288715befe92c734d603f9e6bc3c65d2a8/LICENSE)
is MIT and includes notices for richerfu (2026-present) and Max Brunsfeld (2017). Preserve the complete license when vendoring.

## Native entry and ABI

The npm package contains generated `src/parser.c`, `src/scanner.c`,
`src/tree_sitter` headers and `common/scanner.h`, but omits the Rust Cargo
manifest. The GitHub source contains Rust bindings and a cc build entry.
No unverified crates.io dependency is assumed. The demonstrated option compiles
the unchanged npm C sources with cc, C11, include path `package/src`, preserving
`common/scanner.h` at its relative path. Symbol `tree_sitter_arkts` loads via
`tree_sitter_language::LanguageFn`; ABI **14**, asserted at runtime, is accepted
by **tree-sitter 0.26.10**. Only unused-parameter C warnings occurred.
Production Cargo.toml and Cargo.lock are unchanged. No generation step or Node
runtime is required to compile these existing C sources.

## Observed AST

| Input | AST observed without parse errors |
| --- | --- |
| `@Entry @Component struct Home` | `struct_declaration` with repeated `decorator` fields, `name: type_identifier`, `body: struct_body` |
| `@State count: number = 0` | `public_field_definition`, `decorator`, `name: property_identifier`, `type: type_annotation`, `value: number` |
| `build()` | `method_definition name: property_identifier parameters: formal_parameters body: statement_block` |
| `@Builder function Item()` | `function_declaration` with direct `decorator`, `name: identifier` |
| `import { helper } from './service'` | `import_statement`, `import_clause(named_imports(import_specifier name: identifier))`, `source: string` |
| `helper(); svc.save(1)` | `call_expression function: identifier` and `call_expression function: member_expression object: identifier property: property_identifier` |
| `Column() { Text('x').fontSize(16).opacity(0.6) }` | outer `arkui_component_expression` with `children: arkui_children`; inner single `arkui_component_expression` with repeated `property` / `arguments` field pairs |

The repeated attribute fields must be enumerated, rather than assuming nested
call nodes or taking only the first field. Later extraction must preserve
CodeGraph's ArkTS decorator and attribute-resolution constraints; this probe
adds no resolution or synthesized edges. Damaged `struct Broken { build( {`
returns an ERROR; a Chinese/emoji Text string parses cleanly, with valid UTF-8
slices for every node and root end byte equal to input length.

## Local validation

Ignored local crate `.codex-tmp/native-grammar-probes/arkts`, package
`native_grammar_probe_arkts`, default features, independent `[workspace]`.
Actual tests live in that isolated crate. As in the preceding grammar feasibility
reports, no empty/ignored `tests/upstream_188_test.rs` shell is added: the root
crate has no ArkTS native dependency to exercise. Full executable probe sources
are preserved below for review and reproduction.

Commands, all exit **0**:

- `cargo fmt --all`
- `cargo fmt --manifest-path .codex-tmp/native-grammar-probes/arkts/Cargo.toml --all`
- `cargo test --manifest-path .codex-tmp/native-grammar-probes/arkts/Cargo.toml -- --nocapture`
- `git diff --check`

Result: **3 passed, 0 failed, 0 ignored**, zero doc tests. Tests cover declaration
AST, calls/imports/ArkUI attribute AST, and damaged/multibyte input. Raw output:
`.codex-tmp/native-grammar-probes/arkts/validation.log`; npm metadata, original
archives, extracted sources and Cargo.lock are retained alongside it.
Dynamically acquired **slot 1**,
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1`,
`CARGO_BUILD_BUILD_DIR=$CARGO_TARGET_DIR/build`. Owner record included task,
batch, session and PID; lock released after tests exited, cache retained.
This feasibility task changes no production behavior; TDD is not applicable.
Cancelled real repository replay, Agent A/B, authentication and VM/Docker
acceptance were not run and are not reported as passing.

## Reproduce

Extract [the pinned npm tarball](https://registry.npmjs.org/tree-sitter-arkts/-/tree-sitter-arkts-0.2.0.tgz)
into the probe directory (creating `package/`), create the files below, dynamically
acquire a repository Cargo slot, and run the commands above with its absolute
CARGO_TARGET_DIR. Do not bypass the shared-slot protocol.

### Cargo.toml

```toml
[package]
name = "native_grammar_probe_arkts"
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
        .include("package/src")
        .file("package/src/parser.c")
        .file("package/src/scanner.c")
        .compile("tree-sitter-arkts");
}
```

### src/lib.rs

```rust
#[cfg(test)]
mod tests {
    extern "C" {
        fn tree_sitter_arkts() -> *const ();
    }
    fn parse(s: &str) -> tree_sitter::Tree {
        let language: tree_sitter::Language =
            unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_arkts) }.into();
        assert_eq!(language.abi_version(), 14);
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(s, None).unwrap();
        println!(
            "ABI=14 errors={} {}",
            tree.root_node().has_error(),
            tree.root_node().to_sexp()
        );
        tree
    }
    #[test]
    fn declarations() {
        let t = parse("@Entry @Component struct Home { @State count: number = 0; build() { Text('hello') } } @Builder function Item() { Text('item') }");
        assert!(!t.root_node().has_error());
        let ast = t.root_node().to_sexp();
        for kind in [
            "struct_declaration",
            "struct_body",
            "method_definition",
            "public_field_definition",
            "function_declaration",
            "decorator",
        ] {
            assert!(ast.contains(kind), "{kind}");
        }
    }
    #[test]
    fn calls_and_imports() {
        let t = parse("import { helper } from './service'; function run() { helper(); svc.save(1); } @Component struct Home { build() { Column() { Text('x').fontSize(16).opacity(0.6) } } }");
        assert!(!t.root_node().has_error());
        let ast = t.root_node().to_sexp();
        for kind in [
            "import_statement",
            "call_expression",
            "member_expression",
            "arkui_component_expression",
            "property: (property_identifier)",
            "children:",
        ] {
            assert!(ast.contains(kind), "{kind}");
        }
    }
    #[test]
    fn damaged_and_multibyte() {
        assert!(parse("struct Broken { build( {").root_node().has_error());
        let s = "@Component struct Home { build() { Text('你好🌍') } }";
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
    }
}
```
