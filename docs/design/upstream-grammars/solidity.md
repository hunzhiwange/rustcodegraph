# Solidity native grammar feasibility

Local feasibility confirmed on 2026-11-04; this does not enable Solidity indexing.
Batch 98 has no dependencies. Source was checked at read-only CodeGraph commit
`15483713763f344ff30fb7df92bcf091c7bd9758`.

## Provenance and license

Upstream `src/extraction/grammars.ts` routes Solidity to tree-sitter-wasms;
its package-lock pins **0.1.13**. The npm registry metadata identifies that
package's source commit as `3e88dc9e36b4e8bf752ce53bea61e4b67282a2bb`.
That revision's [package.json](https://github.com/Gregoor/tree-sitter-wasms/blob/3e88dc9e36b4e8bf752ce53bea61e4b67282a2bb/package.json)
pins JoranHonig/tree-sitter-solidity to **b239a95f94cfcc6e7b3e961bc73a28d55e214f02**.
The probe downloaded exactly that commit, not the latest branch or an assumed
crates.io package. This establishes declared build provenance, not byte identity
with the distributed WASM.

The grammar's Cargo.toml declares version **0.0.3**, MIT; its
[LICENSE](https://github.com/JoranHonig/tree-sitter-solidity/blob/b239a95f94cfcc6e7b3e961bc73a28d55e214f02/LICENSE)
contains copyright (c) 2020 Joran Honig and the MIT grant. Future vendoring must
retain that copyright and permission notice. The wrapper package's Unlicense
does not replace the grammar's MIT license.

## Native integration decision

The checked-in `src/parser.c` declares **ABI 14**, exports
`tree_sitter_solidity`, and needs only its accompanying `src/tree_sitter/parser.h`;
no external scanner is compiled by its native build script. Compile this C file
with cc, C11 and `grammar/src` on the include path. Adapt the C function through
`tree_sitter_language::LanguageFn::from_raw` into `tree_sitter::Language`.
The probe demonstrated that tree-sitter **0.26.10** accepts the language and parses
small inputs. The grammar's original Rust binding depends on tree-sitter 0.20.10;
do not import its old Language type into the 0.26 API. Vendoring the pinned C
sources with a modern binding is feasible without grammar regeneration, Node
or WASM runtime. Production Cargo files were not changed. Other-platform builds
were not tested.

## Observed AST contract

| Construct | Observed nodes / fields |
| --- | --- |
| Contract / inheritance | `contract_declaration` has `name`, `body: contract_body`; direct `inheritance_specifier` has `ancestor: user_defined_type` |
| Struct / enum | `struct_declaration` has direct `struct_member` with `type` and `name`; `enum_declaration` has direct `enum_value` children, no body wrapper |
| State / event / error | `state_variable_declaration` has `type`, `name`, optional `value`; `event_definition` uses the grammar's spelling `event_paramater`; `error_declaration` has `name` |
| Functions / modifiers | `function_definition` has `name`, direct `parameter` children, `return_type: return_type_definition`, `body: function_body`; modifier and constructor have dedicated definition nodes |
| Ordinary / member calls | `call_expression function: identifier` or `function: member_expression` with `object` and `property`; arguments are `call_argument` children |
| Guards / emit / revert | `modifier_invocation` is outside the function body; `emit_statement name: identifier`; `revert_statement error: identifier` with `revert_arguments` |
| Named import | `import_directive` has `import_name`, `alias`, `source: string`; use source for module resolution, not imported symbol name |

Broken syntax produced an ERROR node. A Unicode string parsed without errors;
root end offset equalled UTF-8 byte length and every node's byte range sliced the
input safely. No real-project extraction or flow-coverage claim is made.

## Local evidence and reproduction

Untracked `.codex-tmp/native-grammar-probes/solidity/` retains the grammar archive,
extracted sources, npm metadata, build configuration, Cargo.lock, `test.log`, and
`compile-failure.log`. The first probe compile failed because its recursive test
helper omitted a shared Node/TreeCursor lifetime; that helper was corrected.
The final run passed **3 tests, 0 failed, 0 ignored** (plus 0 doc tests).
This feasibility task does not introduce production behavior, so no production
red/green cycle applies. Like the Terraform prerequisite, the executable tests
remain in the isolated probe rather than an empty/ignored
`tests/upstream_098_test.rs` that could falsely suggest production support.

Both build stages dynamically acquired shared slot **1** with atomic mkdir;
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1`.
`CARGO_BUILD_BUILD_DIR=$CARGO_TARGET_DIR/build` overrides the repository's separate
build-dir setting so intermediates also stay in the acquired slot. Each stage
released its lock after all processes exited and retained the cache.

Final commands (default features):

```sh
cargo fmt --all
cargo fmt --manifest-path .codex-tmp/native-grammar-probes/solidity/Cargo.toml --all
cargo test --manifest-path .codex-tmp/native-grammar-probes/solidity/Cargo.toml -- --nocapture
git diff --check
```

All final commands exited **0**. No real repository replay, external model A/B,
CLI authentication, VM or Docker experiment was run, as instructed.

Recreate the probe with the pinned grammar checkout in `grammar/` and these files:

### Cargo.toml

```toml
[package]
name = "native_grammar_probe_solidity"
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
        .compile("tree-sitter-solidity");
}
```

### src/lib.rs

```rust
#[cfg(test)]
mod tests {
    extern "C" {
        fn tree_sitter_solidity() -> *const ();
    }
    fn parse(s: &str) -> tree_sitter::Tree {
        let lang: tree_sitter::Language =
            unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_solidity) }.into();
        assert_eq!(lang.abi_version(), 14);
        let mut p = tree_sitter::Parser::new();
        p.set_language(&lang).unwrap();
        let t = p.parse(s, None).unwrap();
        println!(
            "ABI 14 errors={} {}",
            t.root_node().has_error(),
            t.root_node().to_sexp()
        );
        t
    }
    fn contains(t: &tree_sitter::Tree, kinds: &[&str]) {
        assert!(!t.root_node().has_error());
        for k in kinds {
            assert!(t.root_node().to_sexp().contains(k), "missing {k}");
        }
    }
    #[test]
    fn declarations() {
        let t = parse("pragma solidity ^0.8.0; contract Token is Base { struct Item { uint value; } enum State { On, Off } uint public count; event Changed(uint value); error Denied(); modifier guard() { _; } constructor() {} function transfer(uint amount) public returns (bool) { return true; } }");
        contains(
            &t,
            &[
                "contract_declaration",
                "inheritance_specifier",
                "struct_declaration",
                "struct_member",
                "enum_value",
                "state_variable_declaration",
                "event_definition",
                "error_declaration",
                "modifier_definition",
                "constructor_definition",
                "function_definition",
                "parameter",
                "return_type_definition",
            ],
        );
    }
    #[test]
    fn calls_and_imports() {
        let t = parse("import {Base as Parent} from './Base.sol'; contract Token { function run() public guard { helper(); lib.send(1); emit Changed(1); revert Denied(); } }");
        contains(
            &t,
            &[
                "import_directive",
                "source:",
                "call_expression",
                "member_expression",
                "emit_statement",
                "revert_statement",
                "modifier_invocation",
            ],
        );
    }
    #[test]
    fn damaged_and_multibyte() {
        assert!(parse("contract Broken { function (")
            .root_node()
            .has_error());
        let s = "contract Token { string public label = unicode\"你好🌍\"; }";
        let t = parse(s);
        assert!(!t.root_node().has_error());
        assert_eq!(t.root_node().end_byte(), s.len());
        let mut cursor = t.walk();
        fn check<'a>(n: tree_sitter::Node<'a>, s: &str, c: &mut tree_sitter::TreeCursor<'a>) {
            assert!(s.get(n.byte_range()).is_some());
            for child in n.children(c) {
                check(child, s, &mut child.walk());
            }
        }
        check(t.root_node(), s, &mut cursor);
    }
}
```
