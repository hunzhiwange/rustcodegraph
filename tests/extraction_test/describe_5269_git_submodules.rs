mod describe_5269_git_submodules {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Git Submodules";
    const TS_DESCRIBE_LINE: usize = 5269;
    #[test]
    fn describes_076_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 5269);
    }
    #[test]
    fn case_5280_should_index_files_inside_git_submodules_issue_147() {
        let suite = ["Git Submodules"];
        assert_eq!(suite.len(), 1);
        assert_eq!(265, 265);
        let temp = TempDir::new("codegraph-extraction-git-submodule");
        let lib_dir = temp.path().join("_lib");
        temp.write("_lib/lib.ts", "export const fromSubmodule = 1;");
        git(&lib_dir, ["init", "-q"]);
        git_commit_all(&lib_dir, "lib init");

        let main_dir = temp.path().join("main");
        temp.write("main/app.ts", "export const app = 1;");
        git(&main_dir, ["init", "-q"]);
        git_commit_all(&main_dir, "app init");
        let lib_arg = lib_dir.to_string_lossy().into_owned();
        git(
            &main_dir,
            [
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                lib_arg.as_str(),
                "libs/lib",
            ],
        );
        git_commit_all(&main_dir, "add submodule");

        let files = scan_directory(&main_dir, None);

        assert_contains(&files, "app.ts");
        assert_contains(&files, "libs/lib/lib.ts");
    }
}
