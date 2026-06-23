//! Sync Module Tests
//!
//! Rust port of `__tests__/sync.test.ts`.

mod sync_module {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rustcodegraph::{CodeGraph, IndexOptions};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TempProject {
        path: PathBuf,
    }

    impl TempProject {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos();
            let suffix = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
            let path = path.with_file_name(format!(
                "{}-{suffix}",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .expect("temp directory name should be UTF-8")
            ));
            fs::create_dir_all(&path).unwrap_or_else(|err| {
                panic!("failed to create temp dir {}: {err}", path.display())
            });
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn src(&self) -> PathBuf {
            self.path.join("src")
        }

        fn write_src(&self, name: &str, content: &str) {
            fs::write(self.src().join(name), content)
                .unwrap_or_else(|err| panic!("failed to write src/{name}: {err}"));
        }

        fn remove_src(&self, name: &str) {
            fs::remove_file(self.src().join(name))
                .unwrap_or_else(|err| panic!("failed to remove src/{name}: {err}"));
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            if self.path.exists() {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }

    struct Fixture {
        project: TempProject,
        cg: CodeGraph,
    }

    impl Fixture {
        fn sync_functionality() -> Self {
            let project = TempProject::new("codegraph-sync-func");
            fs::create_dir_all(project.src()).expect("src directory should be created");
            project.write_src("index.ts", "export function hello() { return 'world'; }");

            let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
            let _ = cg.index_all(IndexOptions::default());

            Self { project, cg }
        }

        fn git_based_sync() -> Self {
            let project = TempProject::new("codegraph-git-sync");

            git(project.path(), &["init"]);
            git(project.path(), &["config", "user.email", "test@test.com"]);
            git(project.path(), &["config", "user.name", "Test"]);

            fs::create_dir_all(project.src()).expect("src directory should be created");
            project.write_src("index.ts", "export function hello() { return 'world'; }");

            git(project.path(), &["add", "-A"]);
            git(project.path(), &["commit", "-m", "initial"]);

            let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
            let _ = cg.index_all(IndexOptions::default());

            Self { project, cg }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.cg.destroy();
        }
    }

    fn git(cwd: &Path, args: &[&str]) -> Output {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap_or_else(|err| panic!("failed to run git {args:?}: {err}"));
        assert!(
            output.status.success(),
            "git {args:?} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn contains_path(paths: &[String], expected: &str) -> bool {
        paths.iter().any(|path| path == expected)
    }

    mod sync_functionality {
        use super::*;

        mod get_changed_files {
            use super::*;

            #[test]
            fn should_detect_added_files() {
                let fixture = Fixture::sync_functionality();
                fixture
                    .project
                    .write_src("new.ts", "export function newFunc() { return 42; }");

                let changes = fixture.cg.get_changed_files();

                assert!(contains_path(&changes.added, "src/new.ts"));
                assert_eq!(changes.modified.len(), 0);
                assert_eq!(changes.removed.len(), 0);
            }

            #[test]
            fn should_detect_modified_files() {
                let fixture = Fixture::sync_functionality();
                fixture
                    .project
                    .write_src("index.ts", "export function hello() { return 'modified'; }");

                let changes = fixture.cg.get_changed_files();

                assert_eq!(changes.added.len(), 0);
                assert!(contains_path(&changes.modified, "src/index.ts"));
                assert_eq!(changes.removed.len(), 0);
            }

            #[test]
            fn should_detect_removed_files() {
                let fixture = Fixture::sync_functionality();
                fixture.project.remove_src("index.ts");

                let changes = fixture.cg.get_changed_files();

                assert_eq!(changes.added.len(), 0);
                assert_eq!(changes.modified.len(), 0);
                assert!(contains_path(&changes.removed, "src/index.ts"));
            }
        }

        mod sync {
            use super::*;

            #[test]
            fn should_reindex_added_files() {
                let mut fixture = Fixture::sync_functionality();
                fixture
                    .project
                    .write_src("new.ts", "export function newFunc() { return 42; }");

                let result = fixture.cg.sync(IndexOptions::default());

                assert_eq!(result.files_added, 1);
                assert_eq!(result.files_modified, 0);
                assert_eq!(result.files_removed, 0);

                let nodes = fixture.cg.search_nodes("newFunc", None);
                assert!(!nodes.is_empty());
            }

            #[test]
            fn should_reindex_modified_files() {
                let mut fixture = Fixture::sync_functionality();
                fixture.project.write_src(
                    "index.ts",
                    "export function goodbye() { return 'farewell'; }",
                );

                let result = fixture.cg.sync(IndexOptions::default());

                assert_eq!(result.files_modified, 1);

                let nodes = fixture.cg.search_nodes("goodbye", None);
                assert!(!nodes.is_empty());

                let old_nodes = fixture.cg.search_nodes("hello", None);
                assert_eq!(old_nodes.len(), 0);
            }

            #[test]
            fn should_remove_nodes_from_deleted_files() {
                let mut fixture = Fixture::sync_functionality();
                fixture.project.remove_src("index.ts");

                let result = fixture.cg.sync(IndexOptions::default());

                assert_eq!(result.files_removed, 1);

                let nodes = fixture.cg.search_nodes("hello", None);
                assert_eq!(nodes.len(), 0);
            }

            #[test]
            fn should_report_no_changes_when_nothing_changed() {
                let mut fixture = Fixture::sync_functionality();

                let result = fixture.cg.sync(IndexOptions::default());

                assert_eq!(result.files_added, 0);
                assert_eq!(result.files_modified, 0);
                assert_eq!(result.files_removed, 0);
                assert!(result.files_checked > 0);
            }

            #[test]
            fn should_not_reindex_unchanged_large_files_for_a_small_change() {
                let project = TempProject::new("codegraph-sync-large-unchanged");
                fs::create_dir_all(project.src()).expect("src directory should be created");

                let mut large = String::new();
                for idx in 0..6_000 {
                    large.push_str(&format!(
                        "pub fn large_probe_{idx}() -> usize {{ {idx} }}\n"
                    ));
                }
                project.write_src("large.rs", &large);
                project.write_src("small.ts", "export function smallProbe() { return 1; }");

                let mut cg =
                    CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
                let _ = cg.index_all(IndexOptions::default());

                project.write_src(
                    "small.ts",
                    "export function smallProbeChanged() { return 2; }",
                );
                let result = cg.sync(IndexOptions::default());

                assert_eq!(result.files_added, 0);
                assert_eq!(result.files_modified, 1);
                assert_eq!(result.files_removed, 0);
                assert!(contains_path(
                    result.changed_file_paths.as_deref().unwrap_or(&[]),
                    "src/small.ts"
                ));
                assert!(
                    result.nodes_updated < 20,
                    "sync should update only the changed small file, got {} updated nodes",
                    result.nodes_updated
                );
                assert!(!cg.search_nodes("smallProbeChanged", None).is_empty());
                assert!(!cg.search_nodes("large_probe_5999", None).is_empty());

                cg.destroy();
            }
        }
    }

    mod git_based_sync {
        use super::*;

        #[test]
        fn should_detect_modified_files_via_git() {
            let mut fixture = Fixture::git_based_sync();
            fixture
                .project
                .write_src("index.ts", "export function hello() { return 'modified'; }");

            let result = fixture.cg.sync(IndexOptions::default());

            assert_eq!(result.files_modified, 1);
            assert!(contains_path(
                result.changed_file_paths.as_deref().unwrap_or(&[]),
                "src/index.ts"
            ));
        }

        #[test]
        fn should_detect_new_untracked_files_via_git() {
            let mut fixture = Fixture::git_based_sync();
            fixture
                .project
                .write_src("new.ts", "export function newFunc() { return 42; }");

            let result = fixture.cg.sync(IndexOptions::default());

            assert_eq!(result.files_added, 1);
            assert!(contains_path(
                result.changed_file_paths.as_deref().unwrap_or(&[]),
                "src/new.ts"
            ));

            let nodes = fixture.cg.search_nodes("newFunc", None);
            assert!(!nodes.is_empty());
        }

        #[test]
        fn should_stop_reporting_untracked_files_once_they_are_indexed_issue_206() {
            let mut fixture = Fixture::git_based_sync();
            fixture
                .project
                .write_src("new.ts", "export function newFunc() { return 42; }");

            let first = fixture.cg.sync(IndexOptions::default());
            assert_eq!(first.files_added, 1);

            assert!(!fixture.cg.search_nodes("newFunc", None).is_empty());

            let changes = fixture.cg.get_changed_files();
            assert!(!contains_path(&changes.added, "src/new.ts"));
            assert!(!contains_path(&changes.modified, "src/new.ts"));

            let second = fixture.cg.sync(IndexOptions::default());
            assert_eq!(second.files_added, 0);
            assert_eq!(second.files_modified, 0);
        }

        #[test]
        fn should_re_index_an_untracked_file_when_its_contents_change() {
            let mut fixture = Fixture::git_based_sync();
            fixture
                .project
                .write_src("new.ts", "export function newFunc() { return 42; }");
            let _ = fixture.cg.sync(IndexOptions::default());

            fixture
                .project
                .write_src("new.ts", "export function renamedFunc() { return 7; }");

            let changes = fixture.cg.get_changed_files();
            assert!(contains_path(&changes.modified, "src/new.ts"));

            let result = fixture.cg.sync(IndexOptions::default());
            assert_eq!(result.files_modified, 1);
            assert!(!fixture.cg.search_nodes("renamedFunc", None).is_empty());
            assert_eq!(fixture.cg.search_nodes("newFunc", None).len(), 0);
        }

        #[test]
        fn should_detect_deleted_files_via_git() {
            let mut fixture = Fixture::git_based_sync();
            fixture.project.remove_src("index.ts");

            let result = fixture.cg.sync(IndexOptions::default());

            assert_eq!(result.files_removed, 1);

            let nodes = fixture.cg.search_nodes("hello", None);
            assert_eq!(nodes.len(), 0);
        }

        #[test]
        fn should_skip_files_with_unsupported_extensions() {
            let mut fixture = Fixture::git_based_sync();
            fixture.project.write_src("notes.txt", "just some notes");

            let result = fixture.cg.sync(IndexOptions::default());

            assert_eq!(result.files_added, 0);
            assert_eq!(result.files_modified, 0);
        }

        #[test]
        fn should_report_no_changes_on_clean_working_tree() {
            let mut fixture = Fixture::git_based_sync();

            let result = fixture.cg.sync(IndexOptions::default());

            assert_eq!(result.files_added, 0);
            assert_eq!(result.files_modified, 0);
            assert_eq!(result.files_removed, 0);
            assert!(result.changed_file_paths.is_none());
        }
    }
}
