mod tree_sitter_rust_setup {
    use super::*;

    fn package_json() -> Value {
        let pkg_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("package.json");
        let content = fs::read_to_string(&pkg_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", pkg_path.display()));
        serde_json::from_str(&content).expect("package.json should parse")
    }

    fn cargo_toml() -> String {
        let cargo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        fs::read_to_string(&cargo_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", cargo_path.display()))
    }

    #[test]
    fn production_npm_package_no_longer_depends_on_wasm_tree_sitter() {
        let pkg = package_json();
        let dependencies = pkg["dependencies"]
            .as_object()
            .expect("dependencies should be an object");

        assert!(dependencies.get("web-tree-sitter").is_none());
        assert!(dependencies.get("tree-sitter-wasms").is_none());
    }

    #[test]
    fn should_use_native_tree_sitter_crates_in_cargo_dependencies() {
        let cargo = cargo_toml();

        assert!(cargo.contains("tree-sitter ="));
        assert!(cargo.contains("tree-sitter-typescript"));
        assert!(cargo.contains("tree-sitter-javascript"));
    }
}
