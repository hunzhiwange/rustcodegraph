//! `RUSTCODEGRAPH_WATCH_DEBOUNCE_MS` env override (issue #403).
//!
//! Lets users tune the watcher quiet window from MCP-launched configs without
//! editing the agent's command line -- formatter-on-save chains and large
//! generated outputs benefit from a longer window. Clamped to [100ms, 60s];
//! out-of-range / non-numeric values fall back to the FileWatcher default
//! (2000ms) rather than throwing or silently capping a likely typo.
//!
//! This is the Rust port of `__tests__/mcp-debounce-env.test.ts`.

use rustcodegraph::mcp::engine::parse_debounce_env;

mod parse_debounce_env {
    use super::*;

    #[test]
    fn returns_undefined_for_unset_empty_values() {
        assert_eq!(parse_debounce_env(None), None);
        assert_eq!(parse_debounce_env(Some("")), None);
        assert_eq!(parse_debounce_env(Some("   ")), None);
    }

    #[test]
    fn accepts_integer_values_inside_100_60000() {
        assert_eq!(parse_debounce_env(Some("100")), Some(100));
        assert_eq!(parse_debounce_env(Some("2000")), Some(2000));
        assert_eq!(parse_debounce_env(Some("5000")), Some(5000));
        assert_eq!(parse_debounce_env(Some("60000")), Some(60000));
    }

    #[test]
    fn rejects_out_of_range_values_returns_undefined_lets_default_win() {
        assert_eq!(parse_debounce_env(Some("0")), None);
        assert_eq!(parse_debounce_env(Some("50")), None); // below 100
        assert_eq!(parse_debounce_env(Some("99")), None);
        assert_eq!(parse_debounce_env(Some("60001")), None); // above 60s
        assert_eq!(parse_debounce_env(Some("-500")), None);
    }

    #[test]
    fn rejects_non_integer_non_numeric_values() {
        assert_eq!(parse_debounce_env(Some("abc")), None);
        assert_eq!(parse_debounce_env(Some("500.5")), None);
        assert_eq!(parse_debounce_env(Some("NaN")), None);
        assert_eq!(parse_debounce_env(Some("Infinity")), None);
    }

    #[test]
    fn accepts_scientific_notation_that_resolves_to_an_in_range_integer() {
        // Number('1e3') === 1000, Number.isInteger(1000) === true. Power users
        // who write debounce as 1e3 should not be surprised; the clamp still applies.
        assert_eq!(parse_debounce_env(Some("1e3")), Some(1000));
    }
}
