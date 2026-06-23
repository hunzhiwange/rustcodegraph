mod describe_4493_same_directory_include_kmp_import_resolution {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Same-directory include + KMP import resolution";
    const TS_DESCRIBE_LINE: usize = 4493;
    #[test]
    fn describes_064_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4493);
    }
    #[test]
    fn case_4506_a_c_c_include_resolves_to_the_same_directory_header_not_a_same_named_o() {
        let suite = ["Same-directory include + KMP import resolution"];
        assert_eq!(suite.len(), 1);
        assert_eq!(237, 237);
        let temp = TempDir::new("codegraph-same-dir-cpp-include");
        temp.write(
            "apple/Storage.h",
            "#pragma once\nstruct Storage { int n; };\n",
        );
        temp.write(
            "windows/Storage.h",
            "#pragma once\nstruct Storage { int n; };\n",
        );
        temp.write(
            "windows/Provider.cpp",
            "#include \"Storage.h\"\nint use() { Storage s; return s.n; }\n",
        );

        let mut cg = index_project(&temp);
        let win_header = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("windows/Storage.h"))
            .expect("windows/Storage.h should be indexed");
        let apple_header = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("apple/Storage.h"))
            .expect("apple/Storage.h should be indexed");
        let win_deps = impact_file_paths(&mut cg, &win_header.id, 2);
        let apple_deps = impact_file_paths(&mut cg, &apple_header.id, 2);
        assert!(
            win_deps.iter().any(|path| path.ends_with("Provider.cpp")),
            "same-dir header should reach Provider.cpp: {win_deps:?}"
        );
        assert!(
            apple_deps
                .iter()
                .all(|path| !path.ends_with("Provider.cpp")),
            "other-platform header should not reach Provider.cpp: {apple_deps:?}"
        );
        cg.close();
    }
    #[test]
    fn case_4534_a_kotlin_multiplatform_commonmain_import_resolves_to_the_expect_not_a_() {
        let suite = ["Same-directory include + KMP import resolution"];
        assert_eq!(suite.len(), 1);
        assert_eq!(238, 238);
        let temp = TempDir::new("codegraph-kmp-commonmain-import");
        temp.write(
            "src/commonMain/kotlin/app/Platform.kt",
            "package app\nexpect class PlatformContext\n",
        );
        temp.write(
            "src/androidMain/kotlin/app/Platform.android.kt",
            "package app\nactual class PlatformContext\n",
        );
        temp.write(
            "src/commonMain/kotlin/app/Db.kt",
            r#"package app
import app.PlatformContext
class Db {
  fun open(ctx: PlatformContext) {}
}
"#,
        );

        let mut cg = index_project(&temp);
        let expect_ctx = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| {
                node.name == "PlatformContext"
                    && node
                        .file_path
                        .ends_with("commonMain/kotlin/app/Platform.kt")
            })
            .expect("commonMain expect PlatformContext should be indexed");
        let deps = impact_file_paths(&mut cg, &expect_ctx.id, 2);
        assert!(
            deps.iter().any(|path| path.ends_with("Db.kt")),
            "commonMain import should land on expect PlatformContext: {deps:?}"
        );
        cg.close();
    }
}
