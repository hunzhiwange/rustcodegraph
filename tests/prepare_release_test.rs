//! Unit tests for Rust-owned release-note tooling.
//!
//! The file-facing entry reads CHANGELOG.md and package.json from a working
//! directory, so tests stage real fixtures in temp dirs while exercising the
//! Rust implementation directly.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use rustcodegraph::release::{
    extract_release_notes_from_changelog, extract_release_notes_from_stdin_text,
    prepare_release_in_dir,
};
use serde_json::json;

const HEADER: &str = "# Changelog\n\nSome intro.\n\n";
static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn run(cwd: &Path, args: &[&str]) -> String {
    let version = args.first().copied();
    prepare_release_in_dir(cwd, version)
        .unwrap_or_else(|err| panic!("prepare_release_in_dir {args:?} failed: {err}"))
        .summary
}

fn setup(changelog: &str, version: &str) -> TempDir {
    let dir = TempDir::new("prepare-release");
    fs::write(dir.path().join("CHANGELOG.md"), changelog)
        .unwrap_or_else(|err| panic!("failed to write CHANGELOG.md: {err}"));
    fs::write(
        dir.path().join("package.json"),
        serde_json::to_string(&json!({ "name": "x", "version": version }))
            .expect("package fixture should serialize"),
    )
    .unwrap_or_else(|err| panic!("failed to write package.json: {err}"));
    dir
}

fn setup_default(changelog: &str) -> TempDir {
    setup(changelog, "1.2.3")
}

fn read_changelog(dir: &TempDir) -> String {
    fs::read_to_string(dir.path().join("CHANGELOG.md"))
        .unwrap_or_else(|err| panic!("failed to read CHANGELOG.md: {err}"))
}

fn assert_matches(value: &str, pattern: &str) {
    let regex =
        Regex::new(pattern).unwrap_or_else(|err| panic!("invalid regex {pattern:?}: {err}"));
    assert!(regex.is_match(value), "{value:?} did not match /{pattern}/");
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        for _ in 0..100 {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after Unix epoch")
                .as_nanos();
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "{prefix}-{}-{unique}-{counter}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp dir {}: {err}", path.display()),
            }
        }
        panic!("failed to create a unique temp dir with prefix {prefix}");
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

mod prepare_release_rs {
    use super::*;

    mod case_a_version_block_does_not_yet_exist {
        use super::*;

        #[test]
        fn renames_unreleased_to_version_today_and_adds_a_fresh_empty_unreleased() {
            let dir = setup_default(&format!(
                "{HEADER}## [Unreleased]\n\n### Added\n- New feature foo\n- New feature bar\n\n### Fixed\n- Fixed thing\n\n## [1.2.2] - 2026-01-01\n\n### Added\n- Old entry\n"
            ));
            let out = run(dir.path(), &[]);
            assert_matches(&out, r"renamed \[Unreleased\] to \[1\.2\.3\]");
            let result = read_changelog(&dir);

            // [Unreleased] is now empty and at the top.
            assert_matches(&result, r"## \[Unreleased\]\n\n\n## \[1\.2\.3\]");
            // [1.2.3] gets a date.
            assert_matches(&result, r"## \[1\.2\.3\] - \d{4}-\d{2}-\d{2}");
            // Promoted content lives under [1.2.3].
            let v123_section = result
                .split("## [1.2.3]")
                .nth(1)
                .expect("[1.2.3] section should exist")
                .split("## [1.2.2]")
                .next()
                .expect("[1.2.3] section should precede [1.2.2]");
            assert!(v123_section.contains("### Added"));
            assert!(v123_section.contains("- New feature foo"));
            assert!(v123_section.contains("- New feature bar"));
            assert!(v123_section.contains("### Fixed"));
            assert!(v123_section.contains("- Fixed thing"));
            // [1.2.2] is intact.
            assert!(result.contains("## [1.2.2] - 2026-01-01"));
            assert!(result.contains("- Old entry"));
        }
    }

    mod case_b_version_already_exists_and_unreleased_has_content {
        use super::*;

        #[test]
        fn merges_unreleased_sub_sections_into_the_matching_version_sub_sections() {
            // The v0.9.5 scenario verbatim: sparse [0.9.5] with two Fixed
            // entries, full [Unreleased] above it with Added + more Fixed.
            let dir = setup_default(&format!(
                "{HEADER}## [Unreleased]\n\n### Added\n- Big feature 1\n- Big feature 2\n\n### Fixed\n- Watcher fix\n- Worktree fix\n\n## [1.2.3] - 2026-02-02\n\n### Fixed\n- Old fix A\n- Old fix B\n\n## [1.2.2] - 2026-01-01\n"
            ));
            let out = run(dir.path(), &[]);
            assert_matches(&out, r"merged \d+ Unreleased entries");
            let result = read_changelog(&dir);

            // [Unreleased] is emptied.
            let unrel_section = result
                .split("## [Unreleased]")
                .nth(1)
                .expect("[Unreleased] section should exist")
                .split("## [1.2.3]")
                .next()
                .expect("[Unreleased] section should precede [1.2.3]");
            assert_eq!(unrel_section.trim(), "");

            // [1.2.3] now has BOTH the original Fixed entries AND the
            // Unreleased Fixed entries, plus the new Added sub-section.
            let v123_section = result
                .split("## [1.2.3]")
                .nth(1)
                .expect("[1.2.3] section should exist")
                .split("## [1.2.2]")
                .next()
                .expect("[1.2.3] section should precede [1.2.2]");
            assert!(v123_section.contains("### Added"));
            assert!(v123_section.contains("- Big feature 1"));
            assert!(v123_section.contains("- Big feature 2"));
            assert!(v123_section.contains("### Fixed"));
            assert!(v123_section.contains("- Old fix A"));
            assert!(v123_section.contains("- Old fix B"));
            assert!(v123_section.contains("- Watcher fix"));
            assert!(v123_section.contains("- Worktree fix"));
            // Date on [1.2.3] is preserved (we don't re-stamp it).
            assert!(result.contains("## [1.2.3] - 2026-02-02"));
        }

        #[test]
        fn appends_sub_sections_that_exist_only_in_unreleased_to_the_version_block() {
            let dir = setup_default(&format!(
                "{HEADER}## [Unreleased]\n\n### Security\n- CVE patch\n\n## [1.2.3] - 2026-02-02\n\n### Fixed\n- Old fix\n"
            ));
            run(dir.path(), &[]);
            let result = read_changelog(&dir);
            let v123 = result
                .split("## [1.2.3]")
                .nth(1)
                .expect("[1.2.3] section should exist");
            assert!(v123.contains("### Fixed"));
            assert!(v123.contains("- Old fix"));
            assert!(v123.contains("### Security"));
            assert!(v123.contains("- CVE patch"));
        }
    }

    mod case_c_unreleased_has_no_entries {
        use super::*;

        #[test]
        fn is_a_no_op_when_unreleased_is_empty() {
            let dir = setup_default(&format!(
                "{HEADER}## [Unreleased]\n\n## [1.2.3] - 2026-02-02\n\n### Fixed\n- thing\n"
            ));
            let before = read_changelog(&dir);
            let out = run(dir.path(), &[]);
            assert_matches(&out, r"nothing to do");
            let after = read_changelog(&dir);
            assert_eq!(after, before);
        }

        #[test]
        fn is_a_no_op_when_unreleased_has_only_sub_section_headings_with_no_bullets() {
            let dir = setup_default(&format!(
                "{HEADER}## [Unreleased]\n\n### Added\n\n### Fixed\n\n## [1.2.3] - 2026-02-02\n"
            ));
            let before = read_changelog(&dir);
            let out = run(dir.path(), &[]);
            assert_matches(&out, r"nothing to do");
            let after = read_changelog(&dir);
            assert_eq!(after, before);
        }
    }

    mod idempotency {
        use super::*;

        #[test]
        fn running_twice_produces_the_same_output_as_running_once() {
            let dir = setup_default(&format!(
                "{HEADER}## [Unreleased]\n\n### Added\n- Thing A\n\n## [1.2.2] - 2026-01-01\n\n### Added\n- Old\n"
            ));
            run(dir.path(), &[]); // first run promotes
            let after_first = read_changelog(&dir);
            let out2 = run(dir.path(), &[]); // second run should be a no-op
            let after_second = read_changelog(&dir);
            assert_matches(&out2, r"nothing to do");
            assert_eq!(after_second, after_first);
        }
    }

    mod version_source {
        use super::*;

        #[test]
        fn reads_the_target_version_from_package_json_by_default() {
            let dir = setup(
                &format!("{HEADER}## [Unreleased]\n\n### Added\n- x\n"),
                "9.9.9",
            );
            run(dir.path(), &[]);
            let result = read_changelog(&dir);
            assert!(result.contains("## [9.9.9]"));
        }

        #[test]
        fn accepts_an_explicit_version_argument_that_overrides_package_json() {
            let dir = setup(
                &format!("{HEADER}## [Unreleased]\n\n### Added\n- x\n"),
                "9.9.9",
            );
            run(dir.path(), &["5.5.5"]);
            let result = read_changelog(&dir);
            assert!(result.contains("## [5.5.5]"));
            assert!(!result.contains("## [9.9.9]"));
        }
    }

    mod link_reference {
        use super::*;

        #[test]
        fn appends_a_version_https_link_reference_at_eof_when_promoting_case_a() {
            let dir = setup_default(&format!(
                "{HEADER}## [Unreleased]\n\n### Added\n- x\n\n## [1.2.2] - 2026-01-01\n"
            ));
            run(dir.path(), &[]);
            let result = read_changelog(&dir);
            assert!(result.contains(
                "[1.2.3]: https://github.com/hunzhiwange/rustcodegraph/releases/tag/v1.2.3"
            ));
        }

        #[test]
        fn appends_a_link_reference_when_merging_into_an_existing_version_case_b() {
            let dir = setup_default(&format!(
                "{HEADER}## [Unreleased]\n\n### Added\n- new\n\n## [1.2.3] - 2026-02-02\n\n### Fixed\n- prior\n"
            ));
            run(dir.path(), &[]);
            let result = read_changelog(&dir);
            assert!(result.contains(
                "[1.2.3]: https://github.com/hunzhiwange/rustcodegraph/releases/tag/v1.2.3"
            ));
        }

        #[test]
        fn does_not_double_add_an_existing_link_reference() {
            let ref_line =
                "[1.2.3]: https://github.com/hunzhiwange/rustcodegraph/releases/tag/v1.2.3";
            let dir = setup_default(&format!(
                "{HEADER}## [Unreleased]\n\n### Added\n- x\n\n## [1.2.2] - 2026-01-01\n\n{ref_line}\n"
            ));
            run(dir.path(), &[]);
            let result = read_changelog(&dir);
            let occurrences = result.matches(ref_line).count();
            assert_eq!(occurrences, 1);
        }
    }

    mod extractor_integration {
        use super::*;

        #[test]
        fn the_resulting_version_block_is_what_extract_release_notes_surfaces() {
            // Run prepare, then extract; confirm the output contains all the
            // promoted entries.
            let dir = setup_default(&format!(
                "{HEADER}## [Unreleased]\n\n### Added\n- Feature A\n- Feature B\n\n### Fixed\n- Bug fix\n\n## [1.2.2] - 2026-01-01\n"
            ));
            run(dir.path(), &[]);
            let notes = extract_release_notes_from_changelog(&read_changelog(&dir), "1.2.3")
                .expect("release notes should extract");
            assert!(notes.contains("### Added"));
            assert!(notes.contains("Feature A"));
            assert!(notes.contains("Feature B"));
            assert!(notes.contains("### Fixed"));
            assert!(notes.contains("Bug fix"));
        }
    }
}

mod extract_release_notes_rs {
    use super::*;

    #[test]
    fn extracts_only_the_requested_version_block() {
        let changelog = format!(
            "{HEADER}## [1.2.3] - 2026-02-02\n\n### Fixes\n- Current fix\n\n## [1.2.2] - 2026-01-01\n\n- Old fix\n"
        );
        let notes = extract_release_notes_from_changelog(&changelog, "1.2.3")
            .expect("release notes should extract");

        assert!(notes.contains("## [1.2.3] - 2026-02-02"));
        assert!(notes.contains("- Current fix"));
        assert!(!notes.contains("Old fix"));
    }

    #[test]
    fn joins_hard_wrapped_bullet_continuation_lines() {
        let input = "## [1.2.3]\n\n- A release note that was wrapped\n  onto a second line\n\n";
        let notes = extract_release_notes_from_stdin_text(input);

        assert!(notes.contains("- A release note that was wrapped onto a second line"));
    }

    #[test]
    fn leaves_fenced_code_blocks_verbatim() {
        let input = "## [1.2.3]\n\n```bash\ncodegraph init\n  --flag\n```\n\n";
        let notes = extract_release_notes_from_stdin_text(input);

        assert!(notes.contains("```bash\ncodegraph init\n  --flag\n```"));
    }

    #[test]
    fn normalizes_crlf_input_from_stdin_mode() {
        let input = "## [1.2.3]\r\n\r\n- Wrapped\r\n  line\r\n";
        let notes = extract_release_notes_from_stdin_text(input);

        assert_eq!(notes, "## [1.2.3]\n\n- Wrapped line\n");
    }

    #[test]
    fn reports_missing_versions() {
        let err = extract_release_notes_from_changelog("## [1.2.2]\n\n- Old\n", "1.2.3")
            .expect_err("missing version should fail");

        assert!(err.contains("no '## [1.2.3]' entry found"));
    }
}
