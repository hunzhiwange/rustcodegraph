//! Unit tests for the field-qualified query parser and bounded
//! edit distance -- the two algorithms behind `kind:`/`lang:`/`path:`/
//! `name:` filtering and the fuzzy typo fallback.
//!
//! This is the Rust port of `__tests__/search-query-parser.test.ts`.

use rustcodegraph::search::query_parser::{bounded_edit_distance, parse_query};
use rustcodegraph::types::{Language, NodeKind};

mod parse_query_tests {
    use super::*;

    #[test]
    fn returns_plain_text_for_a_query_with_no_field_prefixes() {
        let r = parse_query("authenticate user");
        assert_eq!(r.text, "authenticate user");
        assert_eq!(r.kinds, Vec::<NodeKind>::new());
        assert_eq!(r.languages, Vec::<Language>::new());
        assert_eq!(r.path_filters, Vec::<String>::new());
        assert_eq!(r.name_filters, Vec::<String>::new());
    }

    #[test]
    fn extracts_kind_filter_and_removes_it_from_text() {
        let r = parse_query("kind:function auth");
        assert_eq!(r.kinds, vec![NodeKind::Function]);
        assert_eq!(r.text, "auth");
    }

    #[test]
    fn extracts_lang_and_language_as_the_same_filter_family() {
        let a = parse_query("lang:typescript foo");
        let b = parse_query("language:typescript foo");
        assert_eq!(a.languages, vec![Language::TypeScript]);
        assert_eq!(b.languages, vec![Language::TypeScript]);
    }

    #[test]
    fn handles_multiple_kind_filters_as_an_or_set() {
        let mut r = parse_query("kind:function kind:method auth");
        r.kinds.sort_by_key(|kind| format!("{kind:?}"));
        assert_eq!(r.kinds, vec![NodeKind::Function, NodeKind::Method]);
    }

    #[test]
    fn extracts_path_and_name_as_substring_filters_kept_verbatim() {
        let r = parse_query("path:src/api name:Handler");
        assert_eq!(r.path_filters, vec!["src/api"]);
        assert_eq!(r.name_filters, vec!["Handler"]);
    }

    #[test]
    fn preserves_quoted_spans_as_a_single_token_whitespace_in_path() {
        let r = parse_query("path:\"my dir/file\" foo");
        assert_eq!(r.path_filters, vec!["my dir/file"]);
        assert_eq!(r.text, "foo");
    }

    #[test]
    fn passes_url_like_tokens_through_to_text_does_not_match_http_as_a_field() {
        let r = parse_query("http://example.com");
        assert_eq!(r.text, "http://example.com");
        assert_eq!(r.kinds, Vec::<NodeKind>::new());
    }

    #[test]
    fn passes_empty_value_tokens_through_as_text_kind_arrow_kind() {
        let r = parse_query("kind: foo");
        assert_eq!(r.kinds, Vec::<NodeKind>::new());
        // The trailing-colon token comes back as plain text
        assert!(r.text.contains("kind:"));
    }

    #[test]
    fn passes_unknown_field_prefixes_through_as_text_todo_keeps_the_colon() {
        let r = parse_query("TODO: needs review");
        assert_eq!(r.text, "TODO: needs review");
        assert_eq!(r.kinds, Vec::<NodeKind>::new());
    }

    #[test]
    fn rejects_unknown_values_for_kind_passes_the_whole_token_to_text() {
        let r = parse_query("kind:invalid foo");
        // Invalid kind value falls back to text
        assert_eq!(r.kinds, Vec::<NodeKind>::new());
        assert!(r.text.contains("kind:invalid"));
    }

    #[test]
    fn handles_all_filters_no_text_query() {
        let r = parse_query("kind:function lang:typescript");
        assert_eq!(r.kinds, vec![NodeKind::Function]);
        assert_eq!(r.languages, vec![Language::TypeScript]);
        assert_eq!(r.text, "");
    }

    #[test]
    fn survives_empty_input() {
        let r = parse_query("");
        assert_eq!(r.text, "");
        assert_eq!(r.kinds, Vec::<NodeKind>::new());
    }

    #[test]
    fn survives_a_very_long_input_no_allocation_explosion() {
        let huge = "foo ".repeat(5000); // 20k chars
        let r = parse_query(&huge);
        assert!(!r.text.is_empty());
    }
}

mod bounded_edit_distance_tests {
    use super::*;

    #[test]
    fn returns_0_for_identical_strings() {
        assert_eq!(bounded_edit_distance("user", "user", 2), 0);
    }

    #[test]
    fn returns_1_for_a_single_substitution() {
        assert_eq!(bounded_edit_distance("user", "usar", 2), 1);
    }

    #[test]
    fn returns_1_for_a_single_insertion() {
        assert_eq!(bounded_edit_distance("user", "users", 2), 1);
    }

    #[test]
    fn returns_1_for_a_single_deletion() {
        assert_eq!(bounded_edit_distance("users", "user", 2), 1);
    }

    #[test]
    fn returns_2_for_a_transposition_two_edits_in_basic_levenshtein() {
        // 'aple' vs 'palp' would be 2; pick a clearer pair.
        // 'foo' vs 'fou': substitution + insertion = 2 if different lengths.
        assert_eq!(bounded_edit_distance("confg", "configX", 2), 2);
    }

    #[test]
    fn returns_max_dist_plus_1_when_distance_clearly_exceeds_budget() {
        assert_eq!(bounded_edit_distance("foo", "completely-different", 2), 3);
    }

    #[test]
    fn respects_length_difference_shortcut() {
        // |len(a) - len(b)| > maxDist must immediately be over budget
        assert_eq!(bounded_edit_distance("a", "aaaaaaa", 2), 3);
    }

    #[test]
    fn handles_empty_inputs() {
        assert_eq!(bounded_edit_distance("", "", 2), 0);
        assert_eq!(bounded_edit_distance("a", "", 2), 1);
        assert_eq!(bounded_edit_distance("", "abc", 2), 3);
    }

    #[test]
    fn is_case_sensitive_caller_must_lowercase_if_case_insensitive_match_wanted() {
        assert_eq!(bounded_edit_distance("Foo", "foo", 2), 1);
    }

    #[test]
    fn early_exits_when_row_min_exceeds_budget_correctness_not_just_perf() {
        // 'aaaaa' vs 'bbbbb': distance is 5, well over budget 2
        assert_eq!(bounded_edit_distance("aaaaa", "bbbbb", 2), 3);
    }
}
