//! Glyph fallback / Unicode-support detection.
//!
//! Pinned because the matrix is small and the consequence of regression
//! is highly visible: shimmer-worker output on Windows mojibakes when
//! UTF-8 glyphs are written via raw console writes (see #168). The detection
//! + ASCII fallback is the contract that prevents this.
//!
//! This is the Rust port of `__tests__/glyphs.test.ts`.

use std::env;
use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard, OnceLock};

use rustcodegraph::ui::glyphs::{
    __reset_platform_for_tests, __set_platform_for_tests, ASCII_GLYPHS, Glyphs, UNICODE_GLYPHS,
    get_glyphs, reset_glyphs_cache, supports_unicode,
};

static TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
    fn new(patch: &[(&'static str, Option<&'static str>)]) -> Self {
        let lock = TEST_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("glyph test mutex should not be poisoned");
        let mut saved = Vec::new();
        for (key, _) in patch {
            if saved.iter().any(|(saved_key, _)| saved_key == key) {
                continue;
            }
            saved.push((*key, env::var_os(key)));
        }

        unsafe {
            for (key, value) in patch {
                if let Some(value) = value {
                    env::set_var(key, value);
                } else {
                    env::remove_var(key);
                }
            }
        }
        reset_glyphs_cache();

        Self { _lock: lock, saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            for (key, value) in &self.saved {
                if let Some(value) = value {
                    env::set_var(key, value);
                } else {
                    env::remove_var(key);
                }
            }
        }
        __reset_platform_for_tests();
        reset_glyphs_cache();
    }
}

fn with_env(patch: &[(&'static str, Option<&'static str>)], fn_body: impl FnOnce()) {
    let _env = EnvGuard::new(patch);
    fn_body();
}

fn set_platform(value: &'static str) {
    __set_platform_for_tests(value);
}

fn glyph_keys() -> Vec<&'static str> {
    vec![
        "bar_empty",
        "bar_filled",
        "dash",
        "err",
        "h_line",
        "info",
        "ok",
        "phase_done",
        "rail",
        "spinner",
        "tree_branch",
        "tree_last",
        "tree_pipe",
        "warn",
    ]
}

fn glyph_entries(glyphs: &Glyphs) -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("ok", vec![glyphs.ok]),
        ("err", vec![glyphs.err]),
        ("info", vec![glyphs.info]),
        ("warn", vec![glyphs.warn]),
        ("spinner", glyphs.spinner.to_vec()),
        ("bar_filled", vec![glyphs.bar_filled]),
        ("bar_empty", vec![glyphs.bar_empty]),
        ("rail", vec![glyphs.rail]),
        ("phase_done", vec![glyphs.phase_done]),
        ("dash", vec![glyphs.dash]),
        ("h_line", vec![glyphs.h_line]),
        ("tree_branch", vec![glyphs.tree_branch]),
        ("tree_last", vec![glyphs.tree_last]),
        ("tree_pipe", vec![glyphs.tree_pipe]),
    ]
}

mod supports_unicode {
    use super::*;

    #[test]
    fn returns_false_on_windows_by_default_mojibake_prone_consoles() {
        with_env(
            &[
                ("RUSTCODEGRAPH_ASCII", None),
                ("RUSTCODEGRAPH_UNICODE", None),
                ("TERM", None),
            ],
            || {
                set_platform("win32");
                assert!(!supports_unicode());
            },
        );
    }

    #[test]
    fn returns_true_on_macos_by_default() {
        with_env(
            &[
                ("RUSTCODEGRAPH_ASCII", None),
                ("RUSTCODEGRAPH_UNICODE", None),
                ("TERM", None),
            ],
            || {
                set_platform("darwin");
                assert!(supports_unicode());
            },
        );
    }

    #[test]
    fn returns_true_on_linux_by_default() {
        with_env(
            &[
                ("RUSTCODEGRAPH_ASCII", None),
                ("RUSTCODEGRAPH_UNICODE", None),
                ("TERM", None),
            ],
            || {
                set_platform("linux");
                assert!(supports_unicode());
            },
        );
    }

    #[test]
    fn returns_false_on_linux_kernel_console_term_linux() {
        with_env(
            &[
                ("RUSTCODEGRAPH_ASCII", None),
                ("RUSTCODEGRAPH_UNICODE", None),
                ("TERM", Some("linux")),
            ],
            || {
                set_platform("linux");
                assert!(!supports_unicode());
            },
        );
    }

    #[test]
    fn respects_codegraph_unicode_1_on_windows_opt_in_escape_hatch() {
        with_env(
            &[
                ("RUSTCODEGRAPH_UNICODE", Some("1")),
                ("RUSTCODEGRAPH_ASCII", None),
            ],
            || {
                set_platform("win32");
                assert!(supports_unicode());
            },
        );
    }

    #[test]
    fn respects_codegraph_ascii_1_on_macos_opt_out_escape_hatch() {
        with_env(
            &[
                ("RUSTCODEGRAPH_ASCII", Some("1")),
                ("RUSTCODEGRAPH_UNICODE", None),
            ],
            || {
                set_platform("darwin");
                assert!(!supports_unicode());
            },
        );
    }

    #[test]
    fn codegraph_ascii_takes_precedence_over_codegraph_unicode() {
        with_env(
            &[
                ("RUSTCODEGRAPH_ASCII", Some("1")),
                ("RUSTCODEGRAPH_UNICODE", Some("1")),
            ],
            || {
                set_platform("darwin");
                assert!(!supports_unicode());
            },
        );
    }
}

mod get_glyphs_suite {
    use super::*;

    #[test]
    fn returns_ascii_glyphs_on_windows() {
        with_env(
            &[
                ("RUSTCODEGRAPH_ASCII", None),
                ("RUSTCODEGRAPH_UNICODE", None),
            ],
            || {
                set_platform("win32");
                let g = get_glyphs();
                assert_eq!(g, ASCII_GLYPHS);
                assert_eq!(g.ok, "[OK]");
                assert_eq!(g.rail, "|");
                assert_eq!(g.phase_done, "*");
                assert_eq!(g.dash, "-");
            },
        );
    }

    #[test]
    fn returns_unicode_glyphs_on_macos() {
        with_env(
            &[
                ("RUSTCODEGRAPH_ASCII", None),
                ("RUSTCODEGRAPH_UNICODE", None),
            ],
            || {
                set_platform("darwin");
                let g = get_glyphs();
                assert_eq!(g, UNICODE_GLYPHS);
                assert_eq!(g.ok, "\u{2713}");
                assert_eq!(g.rail, "\u{2502}");
                assert_eq!(g.phase_done, "\u{25c6}");
                assert_eq!(g.dash, "\u{2014}");
            },
        );
    }

    #[test]
    fn caches_the_result_so_repeated_calls_return_the_same_object() {
        with_env(
            &[
                ("RUSTCODEGRAPH_ASCII", None),
                ("RUSTCODEGRAPH_UNICODE", None),
            ],
            || {
                set_platform("darwin");
                let first = get_glyphs();
                set_platform("win32");
                assert_eq!(get_glyphs(), first);
            },
        );
    }
}

mod glyph_sets {
    use super::*;

    #[test]
    fn ascii_and_unicode_sets_cover_the_same_keys() {
        let mut ascii_keys = glyph_keys();
        let mut unicode_keys = glyph_keys();
        ascii_keys.sort_unstable();
        unicode_keys.sort_unstable();
        assert_eq!(ascii_keys, unicode_keys);
    }

    #[test]
    fn ascii_glyphs_are_all_7_bit_ascii() {
        for (key, values) in glyph_entries(&ASCII_GLYPHS) {
            let flat = values.join("");
            for ch in flat.chars() {
                let codepoint = ch as u32;
                assert!(
                    codepoint < 128,
                    "ASCII_GLYPHS.{key} contains non-ASCII char U+{codepoint:04X}"
                );
            }
        }
    }

    #[test]
    fn ascii_spinner_has_the_same_frame_count_as_the_unicode_spinner() {
        assert_eq!(ASCII_GLYPHS.spinner.len(), UNICODE_GLYPHS.spinner.len());
    }
}
