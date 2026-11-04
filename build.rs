use std::path::Path;

fn main() {
    // Upstream repositories stay unmodified; patched grammars are prepared separately.
    let grammars = [
        ("terraform", ".grammar-build/terraform", true),
        ("solidity", ".grammar-build/solidity", false),
        ("erlang", ".grammar-build/erlang", true),
        ("vbnet", ".grammar-build/vbnet", true),
        ("cobol", ".grammar-build/cobol", true),
        ("cfml", ".grammar-build/cfml/cfml", true),
        ("cfscript", ".grammar-build/cfml/cfscript", true),
        ("arkts", ".grammar-build/arkts", true),
    ];
    // Fail clearly before compiling when a clone or grammar preparation is incomplete.
    for (_, dir, scanner) in grammars {
        for file in ["src/parser.c", "src/tree_sitter/parser.h"]
            .into_iter()
            .chain(scanner.then_some("src/scanner.c"))
        {
            assert!(
                Path::new(dir).join(file).is_file(),
                "Missing grammar source {dir}/{file}. Run `git submodule update --init` \
                 and `npm run prepare:grammars` before building from source."
            );
        }
    }
    for (name, dir, scanner) in grammars {
        println!("cargo:rerun-if-changed={dir}/src");
        if Path::new(dir).join("common").is_dir() {
            println!("cargo:rerun-if-changed={dir}/common");
        }
        let src = format!("{dir}/src");
        let mut build = cc::Build::new();
        build
            .std("c11")
            .include(&src)
            .file(format!("{src}/parser.c"));
        if scanner {
            build.file(format!("{src}/scanner.c"));
        }
        if name == "erlang" && build.get_compiler().is_like_msvc() {
            build.flag("-utf-8");
        }
        build.compile(&format!("tree-sitter-{name}"));
    }
    println!("cargo:rerun-if-changed=.grammar-build/cfml/common");
}
