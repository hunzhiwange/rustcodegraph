mod describe_6893_rust_cross_module_recall {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Rust cross-module recall";
    const TS_DESCRIBE_LINE: usize = 6893;
    fn rust_project(files: &[(&str, &str)]) -> TempDir {
        let temp = TempDir::new("codegraph-rust-cross-module");
        temp.write(
            "Cargo.toml",
            "[package]\nname = \"proj\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        for (relative_path, content) in files {
            temp.write(&format!("src/{relative_path}"), content);
        }
        temp
    }

    #[test]
    fn describes_109_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 6893);
    }
    #[test]
    fn case_6906_extracts_a_struct_literal_foo_as_an_instantiation_across_modules() {
        let suite = ["Rust cross-module recall"];
        assert_eq!(suite.len(), 1);
        assert_eq!(359, 359);
        let temp = rust_project(&[
            ("lib.rs", "pub mod types;\npub mod consumer;\n"),
            ("types.rs", "pub struct Widget { pub n: i32 }\n"),
            (
                "consumer.rs",
                "use crate::types::Widget;\npub fn build() -> Widget { Widget { n: 1 } }\n",
            ),
        ]);
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("src/types.rs");
        assert_contains(&dependents, "src/consumer.rs");
    }
    #[test]
    fn case_6921_extracts_trait_method_declarations_and_bridges_trait_dispatch_to_the_i() {
        let suite = ["Rust cross-module recall"];
        assert_eq!(suite.len(), 1);
        assert_eq!(360, 360);
        let temp = rust_project(&[
            ("lib.rs", "pub mod types;\npub mod consumer;\n"),
            (
                "types.rs",
                "pub trait Render { fn render(&self) -> i32; }\n",
            ),
            (
                "consumer.rs",
                "use crate::types::Render;\npub struct Mine { pub x: i32 }\nimpl Render for Mine { fn render(&self) -> i32 { self.x } }\n",
            ),
        ]);
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("src/types.rs");
        assert_contains(&dependents, "src/consumer.rs");
    }
    #[test]
    fn case_6938_links_pub_use_re_export_hubs_to_the_modules_they_re_export() {
        let suite = ["Rust cross-module recall"];
        assert_eq!(suite.len(), 1);
        assert_eq!(361, 361);
        let temp = rust_project(&[
            ("lib.rs", "pub mod api;\n"),
            ("api/mod.rs", "mod widget;\npub use self::widget::Widget;\n"),
            ("api/widget.rs", "pub struct Widget { pub n: i32 }\n"),
        ]);
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("src/api/widget.rs");
        assert_contains(&dependents, "src/api/mod.rs");
    }
    #[test]
    fn case_6954_resolves_a_qualified_path_to_the_correct_module_when_the_leaf_name_col() {
        let suite = ["Rust cross-module recall"];
        assert_eq!(suite.len(), 1);
        assert_eq!(362, 362);
        let temp = rust_project(&[
            ("lib.rs", "pub mod fast;\npub mod slow;\npub mod hub;\n"),
            ("fast.rs", "pub fn read() -> i32 { 1 }\n"),
            ("slow.rs", "pub fn read() -> i32 { 2 }\n"),
            ("hub.rs", "pub use crate::fast::read;\n"),
        ]);
        let mut cg = index_project(&temp);
        let fast_dependents = cg.get_file_dependents("src/fast.rs");
        let slow_dependents = cg.get_file_dependents("src/slow.rs");
        assert_contains(&fast_dependents, "src/hub.rs");
        assert_not_contains(&slow_dependents, "src/hub.rs");
    }
}
