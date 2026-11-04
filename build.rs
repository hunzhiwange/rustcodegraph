fn main() {
    let src = "vendor/tree-sitter-terraform/src";
    println!("cargo:rerun-if-changed={src}");
    cc::Build::new()
        .std("c11")
        .include(src)
        .file(format!("{src}/parser.c"))
        .file(format!("{src}/scanner.c"))
        .compile("tree-sitter-terraform");
}
