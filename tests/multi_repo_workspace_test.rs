//! Multi-repo workspaces (#514): a directory holding several independent git
//! repositories must index as a whole.
//!
//! Rust port of `__tests__/multi-repo-workspace.test.ts`.

mod multi_repo_workspaces_514 {
    use std::ffi::OsStr;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rustcodegraph::extraction::index::{
        build_scope_ignore, discover_embedded_repo_roots, scan_directory,
    };
    use rustcodegraph::{CodeGraph, IndexOptions};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos();
            let suffix = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "cg-multirepo-{}-{unique}-{suffix}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap_or_else(|err| {
                panic!("failed to create temp dir {}: {err}", path.display())
            });
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn join(&self, relative_path: &str) -> PathBuf {
            self.path.join(relative_path)
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn git<I, S>(cwd: &Path, args: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect::<Vec<_>>();
        let output = Command::new("git")
            .args(&args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|err| panic!("failed to run git {args:?} in {}: {err}", cwd.display()));

        assert!(
            output.status.success(),
            "git {args:?} failed in {}\nstatus: {}\nstderr:\n{}",
            cwd.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// git init + commit everything currently in `dir` as one repo.
    fn make_repo(dir: &Path) {
        git(dir, ["init", "-q"]);
        git(dir, ["add", "-A"]);
        git(
            dir,
            [
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "init",
                "--allow-empty",
            ],
        );
    }

    fn write(file: &Path, content: &str) {
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|err| {
                panic!("failed to create fixture dir {}: {err}", parent.display())
            });
        }
        fs::write(file, content)
            .unwrap_or_else(|err| panic!("failed to write fixture {}: {err}", file.display()));
    }

    fn contains(files: &[String], expected: &str) -> bool {
        files.iter().any(|file| file == expected)
    }

    #[test]
    fn indexes_embedded_repos_hidden_by_the_super_repo_gitignore() {
        let ws = TempWorkspace::new();
        write(
            &ws.join("packages/proj-a/src/auth.ts"),
            "export function login() { return 1; }\n",
        );
        write(
            &ws.join("packages/proj-b/src/billing.ts"),
            "export function charge() { return 2; }\n",
        );
        make_repo(&ws.join("packages/proj-a"));
        make_repo(&ws.join("packages/proj-b"));
        write(&ws.join(".gitignore"), "/packages/\n");
        write(
            &ws.join("tools.ts"),
            "export function tool() { return 0; }\n",
        );
        make_repo(ws.path());

        let files = scan_directory(ws.path(), None);
        assert!(contains(&files, "packages/proj-a/src/auth.ts"), "{files:?}");
        assert!(
            contains(&files, "packages/proj-b/src/billing.ts"),
            "{files:?}"
        );
        assert!(contains(&files, "tools.ts"), "{files:?}");
    }

    #[test]
    fn keeps_respecting_the_parent_gitignore_for_the_parent_own_non_repo_dirs() {
        let ws = TempWorkspace::new();
        write(
            &ws.join("scratch/junk.ts"),
            "export function junk() { return 9; }\n",
        );
        write(
            &ws.join("src/app.ts"),
            "export function app() { return 1; }\n",
        );
        write(&ws.join(".gitignore"), "/scratch/\n");
        make_repo(ws.path());

        let files = scan_directory(ws.path(), None);
        assert!(contains(&files, "src/app.ts"), "{files:?}");
        assert!(
            !files.iter().any(|file| file.starts_with("scratch/")),
            "{files:?}"
        );
    }

    #[test]
    fn never_descends_into_git_repos_inside_node_modules_npm_git_dependencies() {
        let ws = TempWorkspace::new();
        write(
            &ws.join("packages/proj-a/src/auth.ts"),
            "export function login() {}\n",
        );
        make_repo(&ws.join("packages/proj-a"));
        write(
            &ws.join("packages/proj-a/node_modules/inner/src/evil2.ts"),
            "export function evil2() {}\n",
        );
        make_repo(&ws.join("packages/proj-a/node_modules/inner"));
        write(
            &ws.join("node_modules/git-dep/src/evil.ts"),
            "export function evil() {}\n",
        );
        make_repo(&ws.join("node_modules/git-dep"));
        write(&ws.join(".gitignore"), "/packages/\nnode_modules\n");
        make_repo(ws.path());

        let files = scan_directory(ws.path(), None);
        assert!(contains(&files, "packages/proj-a/src/auth.ts"), "{files:?}");
        assert!(
            !files.iter().any(|file| file.contains("node_modules")),
            "{files:?}"
        );
    }

    #[test]
    fn still_indexes_untracked_embedded_repos_193_regression() {
        let ws = TempWorkspace::new();
        write(
            &ws.join("vendor-src/lib/src/util.ts"),
            "export function util() {}\n",
        );
        make_repo(&ws.join("vendor-src/lib"));
        write(&ws.join("main.ts"), "export function main() {}\n");
        make_repo(ws.path());
        git(ws.path(), ["rm", "-r", "--cached", "-q", "vendor-src"]);
        git(
            ws.path(),
            [
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "untrack",
            ],
        );

        let files = scan_directory(ws.path(), None);
        assert!(contains(&files, "vendor-src/lib/src/util.ts"), "{files:?}");
        assert!(contains(&files, "main.ts"), "{files:?}");
    }

    #[test]
    fn skips_nested_git_worktrees_instead_of_indexing_them_as_duplicate_embedded_repos_848() {
        let ws = TempWorkspace::new();
        write(
            &ws.join("src/app.ts"),
            "export function app() { return 1; }\n",
        );
        write(&ws.join(".gitignore"), ".claude/\nvendored/\n");
        make_repo(ws.path());
        git(
            ws.path(),
            [
                "worktree",
                "add",
                "-q",
                ".claude/worktrees/feature",
                "-b",
                "feature",
            ],
        );
        write(
            &ws.join("vendored/lib.ts"),
            "export function vendoredFn() { return 9; }\n",
        );
        make_repo(&ws.join("vendored"));

        let files = scan_directory(ws.path(), None);
        assert!(contains(&files, "src/app.ts"), "{files:?}");
        assert!(
            !files.iter().any(|file| file.contains(".claude/worktrees")),
            "{files:?}"
        );
        assert!(contains(&files, "vendored/lib.ts"), "{files:?}");
    }

    #[test]
    fn non_git_workspace_walks_children_and_respects_each_child_own_gitignore() {
        let ws = TempWorkspace::new();
        write(
            &ws.join("proj-a/src/auth.ts"),
            "export function login() {}\n",
        );
        write(
            &ws.join("proj-a/build/out.ts"),
            "export function generated() {}\n",
        );
        write(&ws.join("proj-a/.gitignore"), "build/\n");
        write(
            &ws.join("proj-b/src/billing.ts"),
            "export function charge() {}\n",
        );
        make_repo(&ws.join("proj-a"));
        make_repo(&ws.join("proj-b"));

        let files = scan_directory(ws.path(), None);
        assert!(contains(&files, "proj-a/src/auth.ts"), "{files:?}");
        assert!(contains(&files, "proj-b/src/billing.ts"), "{files:?}");
        assert!(
            !files.iter().any(|file| file.contains("build/")),
            "{files:?}"
        );
    }

    #[test]
    fn does_not_search_beyond_the_embedded_repo_depth_cap() {
        let ws = TempWorkspace::new();
        let deep = ws.join("pkgs/a/b/c/d/e");
        write(&deep.join("src/deep.ts"), "export function deep() {}\n");
        make_repo(&deep);
        write(&ws.join("main.ts"), "export function main() {}\n");
        write(&ws.join(".gitignore"), "/pkgs/\n");
        make_repo(ws.path());

        let files = scan_directory(ws.path(), None);
        assert!(contains(&files, "main.ts"), "{files:?}");
        assert!(
            !files.iter().any(|file| file.contains("deep.ts")),
            "{files:?}"
        );
    }

    #[test]
    fn discovers_embedded_roots_ignored_untracked_kinds_none_for_non_git_roots() {
        let ws = TempWorkspace::new();
        write(
            &ws.join("packages/proj-a/src/auth.ts"),
            "export function login() {}\n",
        );
        make_repo(&ws.join("packages/proj-a"));
        write(
            &ws.join("vendor-src/lib/util.ts"),
            "export function util() {}\n",
        );
        make_repo(&ws.join("vendor-src/lib"));
        write(&ws.join(".gitignore"), "/packages/\n");
        make_repo(ws.path());
        git(ws.path(), ["rm", "-r", "--cached", "-q", "vendor-src"]);
        git(
            ws.path(),
            [
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "untrack",
            ],
        );

        let roots = discover_embedded_repo_roots(ws.path());
        assert!(
            roots.iter().any(|root| root == "packages/proj-a"),
            "{roots:?}"
        );
        assert!(
            roots.iter().any(|root| root == "vendor-src/lib"),
            "{roots:?}"
        );

        let plain = TempWorkspace::new();
        assert_eq!(
            discover_embedded_repo_roots(plain.path()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn scope_ignore_embedded_files_use_the_child_rules_the_watcher_can_descend_to_them() {
        let ws = TempWorkspace::new();
        write(
            &ws.join("packages/proj-a/src/auth.ts"),
            "export function login() {}\n",
        );
        write(&ws.join("packages/proj-a/.gitignore"), "build/\n");
        make_repo(&ws.join("packages/proj-a"));
        write(&ws.join(".gitignore"), "/packages/\n");
        make_repo(ws.path());

        let scope = build_scope_ignore(ws.path(), None);
        assert!(!scope.ignores("packages/proj-a/src/auth.ts"));
        assert!(scope.ignores("packages/proj-a/build/out.ts"));
        assert!(scope.ignores("packages/stray.ts"));
        assert!(!scope.ignores("packages/"));
        assert!(scope.ignores("node_modules/dep/index.ts"));
        assert!(!scope.ignores("src/app.ts"));
    }

    #[test]
    fn sync_picks_up_a_change_inside_a_gitignored_embedded_repo() {
        let ws = TempWorkspace::new();
        write(
            &ws.join("packages/proj-a/src/auth.ts"),
            "export function login() { return 1; }\n",
        );
        make_repo(&ws.join("packages/proj-a"));
        write(&ws.join(".gitignore"), "/packages/\n");
        make_repo(ws.path());

        let mut cg = CodeGraph::init_sync(ws.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);
        assert!(!cg.search_nodes("login", None).is_empty());

        write(
            &ws.join("packages/proj-a/src/auth.ts"),
            "export function login() { return 1; }\nexport function logout() { return 0; }\n",
        );
        let _ = cg.sync(IndexOptions::default());

        assert!(!cg.search_nodes("logout", None).is_empty());
        cg.destroy();
    }
}
