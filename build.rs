fn main() {
    let src = "vendor/tree-sitter-terraform/src";
    println!("cargo:rerun-if-changed={src}");
    cc::Build::new()
        .std("c11")
        .include(src)
        .file(format!("{src}/parser.c"))
        .file(format!("{src}/scanner.c"))
        .compile("tree-sitter-terraform");
    let solidity_src = "vendor/tree-sitter-solidity/src";
    println!("cargo:rerun-if-changed={solidity_src}");
    cc::Build::new()
        .std("c11")
        .include(solidity_src)
        .file(format!("{solidity_src}/parser.c"))
        .compile("tree-sitter-solidity");
    let erlang_src = "vendor/tree-sitter-erlang/src";
    println!("cargo:rerun-if-changed={erlang_src}");
    let mut erlang_build = cc::Build::new();
    if erlang_build.get_compiler().is_like_msvc() {
        erlang_build.flag("-utf-8");
    }
    erlang_build
        .std("c11")
        .include(erlang_src)
        .file(format!("{erlang_src}/parser.c"))
        .file(format!("{erlang_src}/scanner.c"))
        .compile("tree-sitter-erlang");
    let vbnet_src = "vendor/tree-sitter-vbnet/src";
    println!("cargo:rerun-if-changed={vbnet_src}");
    cc::Build::new()
        .std("c11")
        .include(vbnet_src)
        .file(format!("{vbnet_src}/parser.c"))
        .file(format!("{vbnet_src}/scanner.c"))
        .compile("tree-sitter-vbnet");
    let src = "vendor/tree-sitter-cobol/src";
    println!("cargo:rerun-if-changed={src}");
    cc::Build::new()
        .std("c11")
        .include(src)
        .file(format!("{src}/parser.c"))
        .file(format!("{src}/scanner.c"))
        .compile("tree-sitter-cobol");
    let src = "vendor/tree-sitter-cfml/cfml/src";
    println!("cargo:rerun-if-changed=vendor/tree-sitter-cfml");
    cc::Build::new()
        .std("c11")
        .include(src)
        .file(format!("{src}/parser.c"))
        .file(format!("{src}/scanner.c"))
        .compile("tree-sitter-cfml");
    let src = "vendor/tree-sitter-cfml/cfscript/src";
    cc::Build::new()
        .std("c11")
        .include(src)
        .file(format!("{src}/parser.c"))
        .file(format!("{src}/scanner.c"))
        .compile("tree-sitter-cfscript");
}
