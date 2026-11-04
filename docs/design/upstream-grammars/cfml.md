# CFML native grammar feasibility

Confirmed 2026-11-04, batch 170, no dependencies. This proves a native binding
option; it does not add CFML extraction or establish full language coverage.

## Source and license

Read-only CodeGraph HEAD: `15483713763f344ff30fb7df92bcf091c7bd9758`.
Its `src/extraction/grammars.ts` loads three vendored WASM grammars: cfml,
cfscript, cfquery. No grammar revision or CFML provenance document was found
in that fixed checkout. **The exact source commit used to build those WASM
files remains unknown; the native candidate below is not claimed byte-equivalent.**

Native candidate: [cfmleditor/tree-sitter-cfml](https://github.com/cfmleditor/tree-sitter-cfml/tree/24051c091c9c1a69b36914533aee7aa3286d5526),
commit `24051c091c9c1a69b36914533aee7aa3286d5526`, package version 0.26.44.
Downloaded the GitHub API commit archive after a git clone failed with a partial
transfer; no source modifications or generated grammar were needed.
[LICENSE](https://github.com/cfmleditor/tree-sitter-cfml/blob/24051c091c9c1a69b36914533aee7aa3286d5526/LICENSE)
is MIT, copyright 2014 Gareth Edwards & Gavin Baumanis. Preserve copyright and
permission notices when vendoring. Source's LIMITATIONS.md remains relevant;
these small samples do not establish parity with the upstream WASM version.

## Native entry and ABI

Each of `cfml`, `cfscript`, `cfquery` has generated `src/parser.c` and
`src/scanner.c`. Compile both as C11, include that grammar's `src`, preserve
`common/scanner.h` and sibling headers. Entries are `tree_sitter_cfml`,
`tree_sitter_cfscript`, `tree_sitter_cfquery`. All three load via
`tree_sitter_language::LanguageFn` in tree-sitter **0.26.10**, observed and
asserted ABI **15**. C compilation reports unused parameter warnings only.

Direct path use of the candidate's Rust crate failed dependency resolution
(exit 101): its dev dependency tree-sitter ^0.25 conflicts with 0.26's `links`.
The executable option verified here is a small cc wrapper around the unchanged
C sources, independent of that Cargo package. Do not simply add the tested
source package as a path dependency. Root Cargo.toml and Cargo.lock unchanged.

## Observed AST

| Input | Native AST, no parse errors |
| --- | --- |
| script component/function | `component body: component_body`, `function_declaration name: identifier parameters: formal_parameters body: statement_block`; modifier `access_type`, `parameter_type` |
| tag component/function | `cf_component_open_tag`, `cf_function_tag` with `cf_attribute` / `quoted_cf_attribute_value`, `cf_return_tag`, `cf_component_close_tag` |
| import app.Service | `import_statement source: import_path(identifier, identifier)` |
| svc.save(1) | `call_expression function: member_expression object: identifier property: property_identifier arguments: arguments(number)` |
| helper() | `call_expression function: identifier arguments: arguments` |
| SQL with #getId()# | `query_assignment_expression` → `hash_expression` → `call_expression` |

Broken function syntax returns ERROR. Chinese/emoji string contents parse cleanly,
all AST byte ranges slice valid UTF-8, and root end byte equals input byte length.
The tag grammar keeps script/query bodies separate: later integration must parse
injections with their respective grammar and rebase ranges, and choose cfscript
for bare-script components as CodeGraph does. No automatic injection execution,
full tag extraction, resolver behavior or real repository coverage is proven here.

## Local checks

Ignored local crate `.codex-tmp/native-grammar-probes/cfml`, name
`native_grammar_probe_cfml`, default features, independent `[workspace]`.
Commands: `cargo fmt --all`;
`cargo fmt --manifest-path .codex-tmp/native-grammar-probes/cfml/Cargo.toml --all`;
`cargo test --manifest-path .codex-tmp/native-grammar-probes/cfml/Cargo.toml -- --nocapture`;
`git diff --check`. Final exits **0**, **3 passed, 0 failed, 0 ignored**, zero
doc tests. Initial dependency failure is retained in `initial-failure.log`;
final raw AST/test output in `validation.log`, plus archive and Cargo.lock.

Both stages dynamically acquired **slot 1**,
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1`,
`CARGO_BUILD_BUILD_DIR=$CARGO_TARGET_DIR/build`. Locks released after child
process exit; caches retained. Production behavior is unchanged, so TDD is not
applicable. As with the other feasibility reports, actual native tests live
in the isolated probe; no empty/ignored root integration test shell is added.
Complete reproducible probe sources follow. Download the pinned archive into
`source/` and run Cargo under a dynamically acquired shared slot.

Cancelled real repository replay, Agent A/B, authentication and VM/Docker
acceptance were not run and are not claimed as passing.

### Cargo.toml

```toml
[package]
name = "native_grammar_probe_cfml"
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
    for name in ["cfml", "cfscript", "cfquery"] {
        let dir = format!("source/{name}/src");
        cc::Build::new()
            .std("c11")
            .include(&dir)
            .file(format!("{dir}/parser.c"))
            .file(format!("{dir}/scanner.c"))
            .compile(&format!("tree-sitter-{name}"));
    }
}
```

### src/lib.rs

```rust
#[cfg(test)]
mod tests {
    extern "C" {
        fn tree_sitter_cfml() -> *const ();
        fn tree_sitter_cfscript() -> *const ();
        fn tree_sitter_cfquery() -> *const ();
    }
    fn language(f: unsafe extern "C" fn() -> *const ()) -> tree_sitter::Language {
        unsafe { tree_sitter_language::LanguageFn::from_raw(f) }.into()
    }
    fn parse(source: &str, language: tree_sitter::Language) -> tree_sitter::Tree {
        assert_eq!(language.abi_version(), 15);
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(source, None).unwrap();
        println!(
            "ABI=15 errors={} {}",
            tree.root_node().has_error(),
            tree.root_node().to_sexp()
        );
        tree
    }
    fn script(s: &str) -> tree_sitter::Tree {
        parse(s, language(tree_sitter_cfscript))
    }
    #[test]
    fn declarations() {
        let tree = script(
            "component { public string function greet(required string name) { return name; } }",
        );
        assert!(!tree.root_node().has_error());
        assert!(tree.root_node().to_sexp().contains("function_declaration"));
        let tree = parse(
            "<cfcomponent><cffunction name=\"greet\"><cfreturn \"ok\"></cffunction></cfcomponent>",
            language(tree_sitter_cfml),
        );
        assert!(!tree.root_node().has_error());
    }
    #[test]
    fn calls_and_imports() {
        let tree = script("import app.Service; function run() { svc.save(1); helper(); }");
        assert!(!tree.root_node().has_error());
        let ast = tree.root_node().to_sexp();
        assert!(ast.contains("call_expression"));
        assert!(ast.contains("import_statement"));
        let tree = parse(
            "SELECT id FROM users WHERE id = #getId()#",
            language(tree_sitter_cfquery),
        );
        assert!(!tree.root_node().has_error());
        assert!(tree.root_node().to_sexp().contains("call_expression"));
    }
    #[test]
    fn damaged_and_multibyte() {
        assert!(script("function broken( {").root_node().has_error());
        let s = "function greet() { return \"你好🌍\"; }";
        let tree = script(s);
        assert!(!tree.root_node().has_error());
        assert_eq!(tree.root_node().end_byte(), s.len());
        fn check(n: tree_sitter::Node<'_>, s: &str) {
            assert!(s.get(n.byte_range()).is_some());
            for c in n.children(&mut n.walk()) {
                check(c, s);
            }
        }
        check(tree.root_node(), s);
    }
}
```
