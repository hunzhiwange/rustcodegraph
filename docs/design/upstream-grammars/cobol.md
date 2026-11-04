# COBOL native grammar feasibility

Confirmed 2026-11-04, batch 152, no dependencies. Native parsing is feasible;
this task adds no production COBOL extraction or complete language support.

## Provenance and license

Read-only CodeGraph HEAD: `15483713763f344ff30fb7df92bcf091c7bd9758`.
Its `docs/grammars/tree-sitter-cobol.md` pins
[yutaro-sakamoto/tree-sitter-cobol](https://github.com/yutaro-sakamoto/tree-sitter-cobol/tree/e99dbdc3d800d5fa2796476efd60af91f6b43d93)
at `e99dbdc3d800d5fa2796476efd60af91f6b43d93`, plus its local
`docs/grammars/tree-sitter-cobol.patch` (SHA-256
`1ac407155e29e0b90e34b662f07f0d63ce3a0af310afdd40b4ed5b674b672a1d`).
Git clone stalled; the fixed-commit GitHub codeload archive downloaded successfully.
Apply with `patch -p1` in the extracted source directory. Verify patched scanner
contains `CGWIDE` and the EOF guard; `git apply` inside an ignored nested directory
silently skipped files during this probe and must not be used without verification.

The downloaded LICENSE is MIT, copyright 2016 Max Brunsfeld. Retain the copyright
and permission notice when vendoring. CodeGraph's patch comes from its MIT tree,
copyright 2026 Colby Mchenry; preserve that notice too. No grammar source is
committed here and no crates.io grammar dependency is assumed.

## Native entry and AST

Generate patched grammar with tree-sitter CLI **0.25.10**, then compile generated
`src/parser.c` AND patched `src/scanner.c` using C11 and include directory `src`.
Entry symbol is case-sensitive **tree_sitter_COBOL**; external scanner exports
share that prefix. `LanguageFn::from_raw(...).into()` loads in tree-sitter
**0.26.10**, observed and asserted language ABI **15**. Node/WASM is unnecessary
at runtime. Root Cargo.toml and Cargo.lock are unchanged.

| Input | Observed native AST |
| --- | --- |
| PROGRAM-ID | `start > program_definition > identification_division > program_name` |
| Data declarations | `data_division > working_storage_section > data_description`, children `level_number`, `entry_name`, `picture_clause` |
| Section/paragraph | flat `section_header` / `paragraph_header` under `procedure_division`; extents require reconstruction |
| PERFORM | `perform_statement_call_proc procedure: (perform_procedure (label (qualified_word (WORD))))` |
| Literal CALL | `call_statement x: (string)` |
| COPY | `copy_statement book: (WORD)` |

Declaration and call/import fixtures parse with errors=false. Unterminated CALL
returns an ERROR region. Chinese/emoji in a DISPLAY string parses cleanly; root
end byte equals UTF-8 source length and every node range slices the source safely.
Short final sequence-area input `    .` terminates, producing an empty `start`.
These are small-input checks, not evidence of full COBOL coverage. Fixed format
uses columns 8–72; future integration must port the upstream free-format padding
and CGWIDE sentinel, copybook support, and flat-scope reconstruction deliberately.
Dynamic CALL names must not be guessed as static program targets.

## Reproducible local checks

Ignored crate `.codex-tmp/native-grammar-probes/cobol`, package
`native_grammar_probe_cobol`, isolated `[workspace]`, default features.
Three native tests passed, zero failed/ignored; zero doc tests. Final exit **0**.
All Cargo stages dynamically acquired **slot 1**:
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/rustcodegraph/target/rust-slot-1`;
`CARGO_BUILD_BUILD_DIR=$CARGO_TARGET_DIR/build`. Locks released, caches retained.
Commands: `cargo fmt --all`;
`cargo fmt --manifest-path .codex-tmp/native-grammar-probes/cobol/Cargo.toml --all`;
`cargo test --manifest-path .codex-tmp/native-grammar-probes/cobol/Cargo.toml -- --nocapture`;
`git diff --check`. Final commands exited 0.

Initial compile failed (exit 101): tree-sitter 0.26 removed set_timeout_micros;
the obsolete probe call was removed. A subsequent stale native build was terminated
(SIGTERM, Cargo exit 101) at the EOF fixture; forcing build.rs to rebuild compiled
the verified patched scanner and all tests passed. Production vendoring must add
rerun-if-changed directives for parser/scanner. Raw initial-failure.log,
stale-build.log, generate.log, test.log, archive and Cargo.lock remain locally.
Generator exited 0; C compilation reports signedness warnings only.

The three actual native tests are preserved below rather than adding a root
`tests/upstream_152_test.rs` inventory shell: root has no COBOL dependency or
native link configuration, and a placeholder would not validate parsing.
Recreate the temporary crate from these files, download the pinned archive,
apply the pinned patch, generate, then test under a dynamically acquired slot.
No production behavior changed, so production TDD is inapplicable.
Cancelled real-repository replay, Agent A/B, authentication and VM/Docker checks
were not run and are not reported as passing.

### source/tree-sitter.json

```json
{"grammars":[{"name":"COBOL","camelcase":"COBOL","scope":"source.cobol","path":".","file-types":["cbl","cob","cpy"]}],"metadata":{"version":"0.1.1","license":"MIT","description":"COBOL grammar","links":{"repository":"https://github.com/yutaro-sakamoto/tree-sitter-cobol"}}}
```

### Cargo.toml

```toml
[package]
name="native_grammar_probe_cobol"
version="0.1.0"
edition="2021"
[workspace]
[dependencies]
tree-sitter="=0.26.10"
tree-sitter-language="0.1"
[build-dependencies]
cc="1.2"

```

### build.rs

```rust
fn main() {
    cc::Build::new()
        .std("c11")
        .include("source/src")
        .file("source/src/parser.c")
        .file("source/src/scanner.c")
        .compile("tree-sitter-cobol");
}

```

### src/lib.rs

```rust
#[cfg(test)]
mod tests {
    extern "C" {
        fn tree_sitter_COBOL() -> *const ();
    }
    fn parse(s: &str) -> tree_sitter::Tree {
        let l: tree_sitter::Language =
            unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_COBOL) }.into();
        println!("ABI={}", l.abi_version());
        assert_eq!(l.abi_version(), 15);
        let mut p = tree_sitter::Parser::new();
        p.set_language(&l).unwrap();
        let t = p.parse(s, None).expect("parse completes");
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
    const HEAD: &str = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. DEMO.\n";
    #[test]
    fn declarations() {
        clean(&format!("{HEAD}       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01 WS-GROUP.\n          05 WS-NAME PIC X(10).\n       PROCEDURE DIVISION.\n       MAIN-SEC SECTION.\n       MAIN-PARA.\n           STOP RUN.\n"), &["program_definition", "program_name", "data_description", "level_number", "entry_name", "section_header", "paragraph_header"]);
    }
    #[test]
    fn calls_and_imports() {
        clean(&format!("{HEAD}       PROCEDURE DIVISION.\n       MAIN-PARA.\n           PERFORM WORK-PARA.\n           CALL 'OTHER'.\n           COPY BOOK1.\n       WORK-PARA.\n           STOP RUN.\n"), &["perform_statement_call_proc", "procedure:", "call_statement", "x: (string", "copy_statement", "book:"]);
    }
    #[test]
    fn damaged_and_multibyte() {
        assert!(parse(&format!(
            "{HEAD}       PROCEDURE DIVISION.\n           CALL 'unterminated\n"
        ))
        .root_node()
        .has_error());
        let s = format!("{HEAD}       PROCEDURE DIVISION.\n           DISPLAY '你好🌍'.\n           STOP RUN.\n");
        let t = parse(&s);
        assert!(!t.root_node().has_error());
        assert_eq!(t.root_node().end_byte(), s.len());
        fn check(n: tree_sitter::Node<'_>, s: &str) {
            assert!(s.get(n.byte_range()).is_some());
            for c in n.children(&mut n.walk()) {
                check(c, s);
            }
        }
        check(t.root_node(), &s);
        parse("    ."); // patched scanner must terminate at EOF in sequence area
    }
}

```
