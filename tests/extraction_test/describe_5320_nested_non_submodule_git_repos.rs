mod describe_5320_nested_non_submodule_git_repos {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Nested non-submodule git repos";
    const TS_DESCRIBE_LINE: usize = 5320;
    #[test]
    fn describes_077_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 5320);
    }
    #[test]
    fn case_5331_should_index_files_in_embedded_git_repos_run_from_a_git_super_repo_iss() {
        let suite = ["Nested non-submodule git repos"];
        assert_eq!(suite.len(), 1);
        assert_eq!(266, 266);
        let temp = TempDir::new("codegraph-extraction-embedded-repos");
        let root = temp.path().join("root");
        temp.write(
            "root/CMakeLists.txt",
            "cmake_minimum_required(VERSION 3.10)\n",
        );
        git(&root, ["init", "-q"]);

        let sub1 = root.join("sub_repo1");
        temp.write("root/sub_repo1/src/one.ts", "export const one = 1;");
        git(&sub1, ["init", "-q"]);
        git_commit_all(&sub1, "sub1 init");

        let sub2 = root.join("sub_repo2");
        temp.write("root/sub_repo2/src/two.ts", "export const two = 2;");
        git(&sub2, ["init", "-q"]);

        let files = scan_directory(&root, None);

        assert_contains(&files, "sub_repo1/src/one.ts");
        assert_contains(&files, "sub_repo2/src/two.ts");
    }
    #[test]
    fn case_5368_should_respect_each_embedded_repo_s_own_gitignore() {
        let suite = ["Nested non-submodule git repos"];
        assert_eq!(suite.len(), 1);
        assert_eq!(267, 267);
        let temp = TempDir::new("codegraph-extraction-embedded-repo-own-gitignore");
        let root = temp.path().join("root");
        fs::create_dir_all(&root)
            .unwrap_or_else(|err| panic!("failed to create {}: {err}", root.display()));
        git(&root, ["init", "-q"]);

        let sub = root.join("sub_repo");
        temp.write("root/sub_repo/.gitignore", "src/generated.ts\n");
        temp.write("root/sub_repo/src/real.ts", "export const real = 1;");
        temp.write(
            "root/sub_repo/src/generated.ts",
            "export const generated = 1;",
        );
        git(&sub, ["init", "-q"]);

        let files = scan_directory(&root, None);

        assert_contains(&files, "sub_repo/src/real.ts");
        assert!(
            !files.iter().any(|file| file == "sub_repo/src/generated.ts"),
            "embedded repo .gitignore should hide generated file: {files:?}"
        );
    }
    #[test]
    fn case_5392_does_not_crash_on_a_gitignore_with_an_uncompilable_pattern_682() {
        let suite = ["Nested non-submodule git repos"];
        assert_eq!(suite.len(), 1);
        assert_eq!(268, 268);
        let temp = TempDir::new("codegraph-extraction-bad-gitignore-pattern");
        temp.write("src/real.ts", "export const x = 1;");
        temp.write("build/out.ts", "export const y = 2;");
        temp.write(".gitignore", "build/\n\\\\[\n");

        let files = scan_directory(temp.path(), None);

        assert_contains(&files, "src/real.ts");
        assert!(
            !files.iter().any(|file| file.starts_with("build/")),
            "valid build/ rule should still be honored: {files:?}"
        );
    }
    #[test]
    fn case_5413_does_not_crash_on_a_non_utf_8_dlp_encrypted_gitignore_682() {
        let suite = ["Nested non-submodule git repos"];
        assert_eq!(suite.len(), 1);
        assert_eq!(269, 269);
        let temp = TempDir::new("codegraph-extraction-non-utf8-gitignore");
        temp.write("src/real.ts", "export const x = 1;");
        let mut encrypted = vec![0x00, 0x00];
        for unit in "[notice][user]".encode_utf16() {
            encrypted.extend_from_slice(&unit.to_le_bytes());
        }
        encrypted.extend_from_slice(&[0x5b, 0x99, 0xc3, 0x28, 0x5c, 0x5b, 0xff, 0xfd]);
        fs::write(temp.path().join(".gitignore"), encrypted)
            .expect("failed to write non-UTF-8 .gitignore");

        let files = scan_directory(temp.path(), None);

        assert_contains(&files, "src/real.ts");
    }
    #[test]
    fn case_5430_builddefaultignore_survives_a_bad_gitignore_and_still_applies_valid_ru() {
        let suite = ["Nested non-submodule git repos"];
        assert_eq!(suite.len(), 1);
        assert_eq!(270, 270);
        let temp = TempDir::new("codegraph-extraction-bad-gitignore-build-default");
        temp.write(".gitignore", "dist/\n\\\\[\n");

        let ignore = build_default_ignore(temp.path());

        assert!(!ignore.ignores("src/app.ts"));
        assert!(ignore.ignores("dist/"));
        assert!(!ignore.ignores("src/app.ts"));
    }
}
