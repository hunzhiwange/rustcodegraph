# Erlang native grammar feasibility

Confirmed locally on 2026-11-04 for batch 116 (no dependencies). This report
establishes a native parser prerequisite; it does not enable Erlang indexing.

## Source and license

Read-only CodeGraph source was checked at
`15483713763f344ff30fb7df92bcf091c7bd9758`. Its Erlang extractor declares
WhatsApp/ELP grammar **0.19, ABI 14**. Introduction commit
`6511722250546eac730f972378c006e046aa9193` (2026-07-03) makes the same declaration.
The official [0.19 tag](https://github.com/WhatsApp/tree-sitter-erlang/tree/0.19)
resolves to **836aa2b6c3af2c7cef3f84049b0ed6d44485a870**; the local probe checks
out exactly that revision. Upstream vendors only a WASM, without a grammar
commit/build recipe: this is version-matched source provenance, not proof of
byte-identical WASM provenance. Do not claim its exact build commit is known.
The upstream migration plan warns against the npm package with the same name;
source directly from the official repository, never that npm package.

The pinned [LICENSE](https://github.com/WhatsApp/tree-sitter-erlang/blob/836aa2b6c3af2c7cef3f84049b0ed6d44485a870/LICENSE)
and package.json specify **Apache-2.0**, while Cargo.toml incorrectly declares
MIT. Use the actual Apache license, retain its text and applicable notices,
mark modifications, and check for a NOTICE file when vendoring. Do not treat the
Cargo metadata as an MIT grant. No grammar source is committed by this task.

## Native entry and compatibility

Generated `src/parser.c` defines LANGUAGE_VERSION **14** and exports
`tree_sitter_erlang`. Build both **parser.c and scanner.c**, including `src/`
for the tree_sitter headers, with C11. The official Rust build script uses cc
and adds `-utf-8` on MSVC. No generator, Node or WASM runtime is required.
Use `tree_sitter_language::LanguageFn::from_raw(tree_sitter_erlang).into()` and
`Parser::set_language` with tree-sitter **0.26.10**. The probe checks
`Language::abi_version() == 14` and successfully parses through that runtime.

Directly using the official 0.19 path crate failed dependency resolution
(exit 101): its tree-sitter 0.23 dev-dependency conflicts with 0.26's native
`links` identity. The successful alternative compiles the unchanged generated C
sources in the probe's build.rs and uses the LanguageFn adapter. A future
production integration can vendor these files with licenses, or fix the official
binding dependency metadata in a reviewed local wrapper; do not downgrade the
application runtime. Root Cargo.toml and Cargo.lock were unchanged.

## Observed AST contract

| Syntax | Observed shape / extraction consequence |
| --- | --- |
| Module | `source_file` → `module_attribute name: atom` |
| Export | `export_attribute funs: fa`, with `fun` and `arity: arity value: integer`; visibility is name/arity |
| Function | `fun_decl clause: function_clause`, with `name`, `args: expr_args`, `body: clause_body`; each clause is a separate top-level declaration; probe asserts two forms for run/1's two clauses |
| Records and types | `record_decl name`, `record_field name`; `type_alias`/`opaque name: type_name name`; `pp_define lhs: macro_lhs name` |
| Spec | `spec fun`, `sigs: type_sig`; type expressions can be `call` nodes, so exclude them from executable calls |
| Local call | `call expr: atom args: expr_args` |
| Remote call | `remote module: remote_module module: atom`, `fun: call expr: atom`; module belongs to the wrapper, not the call node |
| Function references | `internal_fun fun, arity`; `external_fun module: module name: atom, fun, arity` |
| Imports | `import_attribute module: atom funs: fa`; includes are `pp_include`/`pp_include_lib file: string` |

Valid declaration/import/call samples have no parse errors. Broken input has an
ERROR node. A Chinese/emoji string parses without errors; root end byte matches
UTF-8 input length and every node range safely slices the source. These tests do
not establish dynamic-dispatch, extraction, resolver or real-project coverage.

## Evidence and reproduction

The ignored `.codex-tmp/native-grammar-probes/erlang/` retains the pinned checkout,
Cargo.lock, native build script, tests, `dependency-failure.log` and `test.log`.
Final result: **3 passed, 0 failed, 0 ignored**, plus 0 doc tests; exit **0**.
No production behavior is added, so a production red/green cycle is inapplicable.
As with the preceding native feasibility tasks, executable tests live in the
isolated crate, not an empty or ignored `tests/upstream_116_test.rs` shell.
The complete probe sources below preserve the checks for recreation.

Both Cargo stages dynamically acquired shared **slot 1** via atomic mkdir:
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1`.
`CARGO_BUILD_BUILD_DIR=$CARGO_TARGET_DIR/build` also keeps intermediates inside
the slot. Locks were released after processes exited; caches retained.

Final commands, all exit 0:

```sh
cargo fmt --all
cargo fmt --manifest-path .codex-tmp/native-grammar-probes/erlang/Cargo.toml --all
cargo test --manifest-path .codex-tmp/native-grammar-probes/erlang/Cargo.toml -- --nocapture
git diff --check
```

The tests exercise the actual native grammar with default features. Cancelled
real-repository replay, external Agent A/B, authentication and VM/Docker checks
were not executed and are not reported as passed.

Recreate the checkout with:

```sh
git clone https://github.com/WhatsApp/tree-sitter-erlang.git grammar
git -C grammar checkout 836aa2b6c3af2c7cef3f84049b0ed6d44485a870
```

### Cargo.toml

```toml
[package]
name = "native_grammar_probe_erlang"
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
        .compile("tree-sitter-erlang");
}
```

### src/lib.rs

```rust
#[cfg(test)]
mod tests {
    extern "C" {
        fn tree_sitter_erlang() -> *const ();
    }
    fn parse(s: &str) -> tree_sitter::Tree {
        let lang: tree_sitter::Language =
            unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_erlang) }.into();
        assert_eq!(lang.abi_version(), 14);
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(s, None).unwrap();
        println!(
            "ABI={} errors={} {}",
            lang.abi_version(),
            tree.root_node().has_error(),
            tree.root_node().to_sexp()
        );
        tree
    }
    fn contains(t: &tree_sitter::Tree, kinds: &[&str]) {
        assert!(!t.root_node().has_error());
        for kind in kinds {
            assert!(t.root_node().to_sexp().contains(kind), "missing {kind}");
        }
    }
    #[test]
    fn declarations() {
        let t = parse("-module(sample).\n-export([run/1]).\n-record(item, {value}).\n-type item() :: #item{}.\n-opaque hidden() :: integer().\n-define(KEY, ok).\n-spec run(integer()) -> integer().\nrun(0) -> 0;\nrun(N) -> N.\n");
        contains(
            &t,
            &[
                "module_attribute",
                "export_attribute",
                "record_decl",
                "record_field",
                "type_alias",
                "opaque",
                "pp_define",
                "spec",
                "fun_decl",
                "function_clause",
                "args:",
                "body:",
            ],
        );
        assert_eq!(
            t.root_node()
                .named_children(&mut t.walk())
                .filter(|n| n.kind() == "fun_decl")
                .count(),
            2
        );
    }
    #[test]
    fn calls_and_imports() {
        let t = parse("-module(sample).\n-import(lists, [reverse/1]).\n-include(\"item.hrl\").\n-include_lib(\"stdlib/include/qlc.hrl\").\nrun(X) -> helper(X), lists:reverse(X), fun helper/1, fun lists:reverse/1.\n");
        contains(
            &t,
            &[
                "import_attribute",
                "module:",
                "pp_include",
                "pp_include_lib",
                "file:",
                "call",
                "remote",
                "internal_fun",
                "external_fun",
            ],
        );
    }
    #[test]
    fn damaged_and_multibyte() {
        assert!(parse("-module(broken). run( ->").root_node().has_error());
        let s = "-module(sample).\nrun() -> \"你好🌍\".\n";
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
