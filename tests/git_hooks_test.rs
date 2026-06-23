//! Git Sync Hooks Tests
//!
//! Rust port of `__tests__/git-hooks.test.ts`.

mod git_sync_hooks {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rustcodegraph::sync::git_hooks::{
        DEFAULT_SYNC_HOOKS, GitHookName, install_git_sync_hook, is_git_repo,
        is_sync_hook_installed, remove_git_sync_hook,
    };

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TempRepo {
        path: PathBuf,
    }

    impl TempRepo {
        fn new() -> Self {
            for _ in 0..100 {
                let unique = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system time should be after Unix epoch")
                    .as_nanos();
                let suffix = NEXT_TEMP_ID.fetch_add(1, Ordering::SeqCst);
                let path = std::env::temp_dir().join(format!(
                    "codegraph-githooks-{}-{unique}-{suffix}",
                    std::process::id()
                ));
                if fs::create_dir(&path).is_ok() {
                    return Self { path };
                }
            }
            panic!("failed to create unique temp repo directory");
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            if self.path.exists() {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }

    fn git_init(dir: &Path) {
        let output = Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|err| panic!("failed to run git init in {}: {err}", dir.display()));

        assert!(
            output.status.success(),
            "git init failed in {}\nstatus: {}\nstderr:\n{}",
            dir.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_config(dir: &Path, key: &str, value: &str) {
        let output = Command::new("git")
            .args(["config", key, value])
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|err| {
                panic!(
                    "failed to run git config {key} {value} in {}: {err}",
                    dir.display()
                )
            });

        assert!(
            output.status.success(),
            "git config {key} {value} failed in {}\nstatus: {}\nstderr:\n{}",
            dir.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    fn is_executable(file: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;

        fs::metadata(file)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    fn is_executable(_file: &Path) -> bool {
        true
    }

    fn write_executable(file: &Path, content: &str) {
        fs::write(file, content)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", file.display()));
        chmod_executable(file);
    }

    #[cfg(unix)]
    fn chmod_executable(file: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(file)
            .unwrap_or_else(|err| panic!("failed to stat {}: {err}", file.display()))
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(file, permissions)
            .unwrap_or_else(|err| panic!("failed to chmod {}: {err}", file.display()));
    }

    #[cfg(not(unix))]
    fn chmod_executable(_file: &Path) {}

    #[test]
    fn installs_all_default_hooks_executable_invoking_codegraph_sync() {
        let repo = TempRepo::new();
        git_init(repo.path());

        let result = install_git_sync_hook(repo.path(), None);

        let mut installed = result.installed;
        installed.sort();
        let mut expected = DEFAULT_SYNC_HOOKS.to_vec();
        expected.sort();
        assert_eq!(installed, expected);
        assert_eq!(result.skipped, None);

        for hook in DEFAULT_SYNC_HOOKS {
            let file = repo.path().join(".git").join("hooks").join(hook.as_str());
            assert!(file.exists());
            let body = fs::read_to_string(&file)
                .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
            assert!(body.contains("rustcodegraph sync"));
            assert!(body.contains("command -v rustcodegraph"));
            assert!(is_executable(&file));
        }
        assert!(is_sync_hook_installed(repo.path(), None));
    }

    #[test]
    fn is_idempotent_re_install_does_not_duplicate_the_block() {
        let repo = TempRepo::new();
        git_init(repo.path());
        install_git_sync_hook(repo.path(), None);
        install_git_sync_hook(repo.path(), None);

        let file = repo.path().join(".git").join("hooks").join("post-commit");
        let body = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        let occurrences = body.matches("# >>> rustcodegraph sync hook >>>").count();
        assert_eq!(occurrences, 1);
    }

    #[test]
    fn preserves_a_pre_existing_user_hook_and_appends_our_block() {
        let repo = TempRepo::new();
        git_init(repo.path());
        let file = repo.path().join(".git").join("hooks").join("post-commit");
        write_executable(&file, "#!/bin/sh\necho \"my custom hook\"\n");

        install_git_sync_hook(repo.path(), Some(&[GitHookName::PostCommit]));

        let body = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        assert!(body.contains("echo \"my custom hook\""));
        assert!(body.contains("rustcodegraph sync"));
    }

    #[test]
    fn remove_strips_our_block_deletes_a_hook_that_was_only_ours() {
        let repo = TempRepo::new();
        git_init(repo.path());
        install_git_sync_hook(repo.path(), Some(&[GitHookName::PostCommit]));
        let file = repo.path().join(".git").join("hooks").join("post-commit");
        assert!(file.exists());

        let result = remove_git_sync_hook(repo.path(), Some(&[GitHookName::PostCommit]));

        assert_eq!(result.installed, vec![GitHookName::PostCommit]);
        assert!(!file.exists());
        assert!(!is_sync_hook_installed(repo.path(), None));
    }

    #[test]
    fn remove_keeps_user_content_when_the_hook_is_shared() {
        let repo = TempRepo::new();
        git_init(repo.path());
        let file = repo.path().join(".git").join("hooks").join("post-commit");
        write_executable(&file, "#!/bin/sh\necho \"keep me\"\n");
        install_git_sync_hook(repo.path(), Some(&[GitHookName::PostCommit]));

        remove_git_sync_hook(repo.path(), Some(&[GitHookName::PostCommit]));

        assert!(file.exists());
        let body = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        assert!(body.contains("echo \"keep me\""));
        assert!(!body.contains("rustcodegraph sync"));
    }

    #[test]
    fn honors_core_hooks_path() {
        let repo = TempRepo::new();
        git_init(repo.path());
        let custom_hooks = repo.path().join(".husky");
        fs::create_dir(&custom_hooks)
            .unwrap_or_else(|err| panic!("failed to create {}: {err}", custom_hooks.display()));
        git_config(repo.path(), "core.hooksPath", ".husky");

        let result = install_git_sync_hook(repo.path(), Some(&[GitHookName::PostCommit]));

        let expected_hooks_dir = path_string(&custom_hooks);
        assert_eq!(
            result.hooks_dir.as_deref(),
            Some(expected_hooks_dir.as_str())
        );
        assert!(custom_hooks.join("post-commit").exists());
        assert!(
            !repo
                .path()
                .join(".git")
                .join("hooks")
                .join("post-commit")
                .exists()
        );
    }

    #[test]
    fn skips_cleanly_when_not_a_git_repository() {
        let repo = TempRepo::new();

        assert!(!is_git_repo(repo.path()));
        let result = install_git_sync_hook(repo.path(), None);
        assert_eq!(result.installed, Vec::<GitHookName>::new());
        assert_eq!(result.hooks_dir, None);
        assert!(
            result
                .skipped
                .as_deref()
                .is_some_and(|skipped| skipped.contains("not a git repository"))
        );
        assert!(!is_sync_hook_installed(repo.path(), None));
    }

    fn path_string(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }
}
