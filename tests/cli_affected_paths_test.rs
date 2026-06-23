//! `codegraph affected` input-path normalization (#825).
//!
//! The index stores project-relative, forward-slash paths. A user or wrapping
//! script may pass a `./`-prefixed path or an absolute path; all spellings must
//! resolve the same affected test file.

mod codegraph_affected_input_path_normalization_825 {
    use std::ffi::OsStr;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rustcodegraph::{CodeGraph, IndexOptions};

    struct TempProject {
        path: PathBuf,
    }

    impl TempProject {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "codegraph-affected-paths-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("temp project directory should be created");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn codegraph_bin() -> &'static str {
        env!("CARGO_BIN_EXE_rustcodegraph")
    }

    fn run_success(mut command: Command, label: &str) -> Output {
        let debug = format!("{command:?}");
        let output = command
            .output()
            .unwrap_or_else(|err| panic!("failed to run {label} ({debug}): {err}"));
        assert!(
            output.status.success(),
            "{label} failed ({debug})\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn affected(project_root: &Path, arg: impl AsRef<OsStr>) -> Vec<String> {
        let mut command = Command::new(codegraph_bin());
        command
            .arg("affected")
            .arg(arg)
            .arg("--quiet")
            .arg("-p")
            .arg(project_root)
            .env("RUSTCODEGRAPH_NO_DAEMON", "1");
        let output = run_success(command, "codegraph affected");
        String::from_utf8(output.stdout)
            .expect("affected output should be UTF-8")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    }

    struct Fixture {
        project: TempProject,
        cg: CodeGraph,
    }

    impl Fixture {
        fn path(&self) -> &Path {
            self.project.path()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.cg.close();
        }
    }

    fn fixture_project() -> Fixture {
        let project = TempProject::new();
        let src = project.path().join("src");
        fs::create_dir_all(&src).expect("src directory should be created");
        // util.ts <- helper.ts <- helper.test.ts (transitive test dependency)
        fs::write(
            src.join("util.ts"),
            "export function util(x: number){ return x + 1; }\n",
        )
        .expect("util fixture should be written");
        fs::write(
            src.join("helper.ts"),
            "import { util } from './util';\nexport function helper(){ return util(1); }\n",
        )
        .expect("helper fixture should be written");
        fs::write(
            src.join("helper.test.ts"),
            "import { helper } from './helper';\ntest('t', () => helper());\n",
        )
        .expect("test fixture should be written");
        let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(
            result.success,
            "index_all should succeed, errors: {:?}",
            result.errors
        );
        Fixture { project, cg }
    }

    #[test]
    fn bare_relative_dot_prefixed_and_absolute_paths_all_resolve_the_same_affected_test() {
        let fixture = fixture_project();
        let expected = vec!["src/helper.test.ts".to_owned()];

        // Baseline that always worked.
        assert_eq!(affected(fixture.path(), "src/util.ts"), expected);
        // Both of these returned no test file before the normalization fix.
        assert_eq!(affected(fixture.path(), "./src/util.ts"), expected);
        assert_eq!(
            affected(fixture.path(), fixture.path().join("src/util.ts")),
            expected
        );
    }
}
