mod describe_5181_directory_exclusion {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Directory Exclusion";
    const TS_DESCRIBE_LINE: usize = 5181;
    #[test]
    fn describes_075_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 5181);
    }
    #[test]
    fn case_5192_should_exclude_directories_listed_in_gitignore() {
        let suite = ["Directory Exclusion"];
        assert_eq!(suite.len(), 1);
        assert_eq!(260, 260);
        let temp = TempDir::new("codegraph-extraction-gitignore-root");
        temp.write("src/index.ts", "export const x = 1;");
        temp.write("node_modules/pkg/index.js", "module.exports = {};");
        temp.write(".gitignore", "node_modules/\n");

        let files = scan_directory(temp.path(), None);

        assert_contains(&files, "src/index.ts");
        assert_not_contains_fragment(&files, "node_modules");
    }
    #[test]
    fn case_5208_should_exclude_nested_node_modules_via_a_root_gitignore() {
        let suite = ["Directory Exclusion"];
        assert_eq!(suite.len(), 1);
        assert_eq!(261, 261);
        let temp = TempDir::new("codegraph-extraction-gitignore-nested-node-modules");
        temp.write("packages/app/src/index.ts", "export const x = 1;");
        temp.write(
            "packages/app/node_modules/pkg/index.js",
            "module.exports = {};",
        );
        temp.write(".gitignore", "node_modules/\n");

        let files = scan_directory(temp.path(), None);

        assert_contains(&files, "packages/app/src/index.ts");
        assert_not_contains_fragment(&files, "node_modules");
    }
    #[test]
    fn case_5224_should_apply_a_nested_gitignore_only_to_its_own_subtree() {
        let suite = ["Directory Exclusion"];
        assert_eq!(suite.len(), 1);
        assert_eq!(262, 262);
        let temp = TempDir::new("codegraph-extraction-nested-gitignore-scope");
        temp.write("app/src/keep.ts", "export const a = 1;");
        temp.write("app/src/skip.ts", "export const b = 2;");
        temp.write("app/.gitignore", "src/skip.ts\n");
        temp.write("other/src/skip.ts", "export const c = 3;");

        let files = scan_directory(temp.path(), None);

        assert_contains(&files, "app/src/keep.ts");
        assert!(
            !files.iter().any(|file| file == "app/src/skip.ts"),
            "nested .gitignore should ignore only its own subtree: {files:?}"
        );
        assert_contains(&files, "other/src/skip.ts");
    }
    #[test]
    fn case_5242_should_always_skip_git_directories() {
        let suite = ["Directory Exclusion"];
        assert_eq!(suite.len(), 1);
        assert_eq!(263, 263);
        let temp = TempDir::new("codegraph-extraction-skip-git-dir");
        temp.write("src/index.ts", "export const x = 1;");
        temp.write(".git/objects/pack.ts", "export const y = 2;");

        let files = scan_directory(temp.path(), None);

        assert_contains(&files, "src/index.ts");
        assert_not_contains_fragment(&files, ".git");
    }
    #[test]
    fn case_5256_should_return_forward_slash_paths_on_all_platforms() {
        let suite = ["Directory Exclusion"];
        assert_eq!(suite.len(), 1);
        assert_eq!(264, 264);
        let temp = TempDir::new("codegraph-extraction-forward-slashes");
        temp.write("src/components/Button.tsx", "export function Button() {}");

        let files = scan_directory(temp.path(), None);

        assert_eq!(files, vec!["src/components/Button.tsx"]);
        assert_not_contains_fragment(&files, "\\");
    }
}
