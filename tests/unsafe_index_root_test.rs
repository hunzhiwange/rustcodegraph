//! Guard for #845: the installer / `init` / `index` must refuse the home
//! directory and filesystem roots, which would otherwise index the entire tree
//! (multi-GB index, watcher churn, pre-1.0 macOS fd exhaustion that crashed the
//! machine). The classic trigger was running the installer from `$HOME`.
//!
//! This is the Rust port of `__tests__/unsafe-index-root.test.ts`.

mod unsafe_index_root_reason {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rustcodegraph::directory::unsafe_index_root_reason;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after Unix epoch")
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
            fs::create_dir(&path).unwrap_or_else(|err| {
                panic!("failed to create temp dir {}: {err}", path.display())
            });
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn home_dir() -> PathBuf {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .or_else(|| {
                let drive = std::env::var_os("HOMEDRIVE")?;
                let path = std::env::var_os("HOMEPATH")?;
                let mut joined = PathBuf::from(drive);
                joined.push(path);
                Some(joined.into_os_string())
            })
            .map(PathBuf::from)
            .expect("home directory should be available")
    }

    #[test]
    fn flags_the_home_directory() {
        let reason = unsafe_index_root_reason(home_dir()).expect("home should be unsafe");

        assert!(reason.contains("home"), "{reason}");
    }

    #[test]
    fn flags_a_parent_of_the_home_directory_broader_than_home() {
        let home = home_dir();
        let parent = home.parent().expect("home directory should have a dirname");

        // dirname(home) is either a parent of home or, for a root-level home like
        // `/root`, the filesystem root; both are unsafe.
        assert!(unsafe_index_root_reason(parent).is_some());
    }

    #[test]
    #[cfg_attr(windows, ignore = "process.platform !== 'win32'")]
    fn flags_the_posix_filesystem_root() {
        let reason = unsafe_index_root_reason(Path::new("/"))
            .expect("POSIX filesystem root should be unsafe");

        assert!(reason.contains("filesystem root"), "{reason}");
    }

    #[test]
    fn allows_a_normal_project_directory() {
        let dir = TempDir::new("cg-unsafe");

        assert_eq!(unsafe_index_root_reason(dir.path()), None);

        // ...and a nested subdir of it.
        let nested = dir.path().join("packages").join("app");
        fs::create_dir_all(&nested).unwrap_or_else(|err| {
            panic!("failed to create nested dir {}: {err}", nested.display())
        });
        assert_eq!(unsafe_index_root_reason(&nested), None);
    }

    #[test]
    fn matches_the_home_directory_case_insensitively_on_macos_windows() {
        if !cfg!(target_os = "macos") && !cfg!(windows) {
            return;
        }

        // The FS is case-insensitive there, so an upper-cased home path must still flag.
        assert!(unsafe_index_root_reason(home_dir().to_string_lossy().to_uppercase()).is_some());
    }
}
