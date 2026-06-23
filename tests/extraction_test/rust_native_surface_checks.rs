mod rust_native_surface_checks {
    use super::*;

    #[test]
    fn language_detection_matches_the_typescript_extension_table() {
        assert_detected_language("src/index.ts", None, Language::TypeScript);
        assert_detected_language("components/Button.tsx", None, Language::Tsx);
        assert_detected_language("index.js", None, Language::JavaScript);
        assert_detected_language("App.jsx", None, Language::Jsx);
        assert_detected_language("config.mjs", None, Language::JavaScript);
        assert_detected_language("main.py", None, Language::Python);
        assert_detected_language("main.go", None, Language::Go);
        assert_detected_language("lib.rs", None, Language::Rust);
        assert_detected_language("Main.java", None, Language::Java);
        assert_detected_language("main.c", None, Language::C);
        assert_detected_language("utils.h", None, Language::C);
        assert_detected_language("main.cpp", None, Language::Cpp);
        assert_detected_language("class.hpp", None, Language::Cpp);
        assert_detected_language("Program.cs", None, Language::CSharp);
        assert_detected_language("index.php", None, Language::Php);
        assert_detected_language("app.rb", None, Language::Ruby);
        assert_detected_language("ViewController.swift", None, Language::Swift);
        assert_detected_language("MainActivity.kt", None, Language::Kotlin);
        assert_detected_language("build.gradle.kts", None, Language::Kotlin);
        assert_detected_language("main.dart", None, Language::Dart);
        assert_detected_language("AppDelegate.m", None, Language::ObjC);
        assert_detected_language("ViewController.mm", None, Language::ObjC);
        assert_detected_language(
            "Foo.h",
            Some("@interface Foo : NSObject\n@end\n"),
            Language::ObjC,
        );
        assert_detected_language(
            "stdio.h",
            Some("#ifndef STDIO_H\nvoid printf();\n#endif\n"),
            Language::C,
        );
        assert_detected_language("styles.css", None, Language::Unknown);
        assert_detected_language("data.json", None, Language::Unknown);
    }

    #[test]
    fn language_support_reports_the_same_core_languages() {
        assert_language_support(Language::TypeScript, true);
        assert_language_support(Language::Python, true);
        assert_language_support(Language::Go, true);
        assert_language_support(Language::Unknown, false);
        assert_supported_languages_include(&[
            Language::TypeScript,
            Language::JavaScript,
            Language::Python,
            Language::Go,
            Language::Rust,
            Language::Java,
            Language::CSharp,
            Language::Php,
            Language::Ruby,
            Language::Swift,
            Language::Kotlin,
            Language::Dart,
            Language::Pascal,
            Language::Scala,
            Language::Lua,
            Language::Luau,
            Language::ObjC,
            Language::R,
        ]);
    }

    #[test]
    fn path_normalization_matches_typescript_helper() {
        assert_eq!(
            normalize_path("gui\\node_modules\\foo"),
            "gui/node_modules/foo"
        );
        assert_eq!(
            normalize_path("src\\components\\Button.tsx"),
            "src/components/Button.tsx"
        );
        assert_eq!(
            normalize_path("src/components/Button.tsx"),
            "src/components/Button.tsx"
        );
        assert_eq!(normalize_path(""), "");
    }

    #[test]
    fn scan_directory_honors_root_gitignore_and_forward_slashes() {
        let temp = TempDir::new("codegraph-extraction-scan");
        temp.write("src/index.ts", "export const x = 1;");
        temp.write("node_modules/pkg/index.js", "module.exports = {};");
        temp.write("packages/app/src/Button.tsx", "export function Button() {}");
        temp.write(
            "packages/app/node_modules/pkg/index.js",
            "module.exports = {};",
        );
        temp.write(".gitignore", "node_modules/\n");

        let files = scan_directory(temp.path(), None);

        assert_contains(&files, "src/index.ts");
        assert_contains(&files, "packages/app/src/Button.tsx");
        assert_not_contains_fragment(&files, "node_modules");
        assert!(
            files.iter().all(|file| !file.contains('\\')),
            "files: {files:?}"
        );
    }

    #[test]
    fn build_default_ignore_survives_bad_gitignore_lines_and_keeps_valid_rules() {
        let temp = TempDir::new("codegraph-extraction-ignore");
        temp.write(".gitignore", "dist/\n\\[\n");
        let ignore = build_default_ignore(temp.path());

        assert!(!ignore.ignores("src/app.ts"));
        assert!(ignore.ignores("dist/"));
        assert!(!ignore.ignores("src/app.ts"));
    }
}
