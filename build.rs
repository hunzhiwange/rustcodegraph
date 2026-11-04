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
}
