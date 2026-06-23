//! Rust port of `__tests__/swift-objc-bridge.test.ts`.

use rustcodegraph::resolution::swift_objc_bridge::{
    ObjcAccessors, detect_explicit_objc_name, is_objc_exposed, objc_accessors_for_swift_property,
    objc_selector_for_swift_init, objc_selector_for_swift_method,
    swift_base_names_for_objc_selector,
};

fn labels(values: &[Option<&str>]) -> Vec<Option<String>> {
    values
        .iter()
        .map(|value| value.map(str::to_string))
        .collect()
}

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

fn accessors(getter: &str, setter: &str) -> Option<ObjcAccessors> {
    Some(ObjcAccessors {
        getter: getter.to_string(),
        setter: setter.to_string(),
    })
}

mod swift_to_objc_selector_bridging_auto_name_rules {
    use super::*;

    mod objc_selector_for_swift_method {
        use super::*;

        #[test]
        fn no_parameters_bare_base_name() {
            assert_eq!(
                objc_selector_for_swift_method("play", &labels(&[]), None).as_deref(),
                Some("play")
            );
        }

        #[test]
        fn single_underscore_param_base_plus_colon() {
            assert_eq!(
                objc_selector_for_swift_method("play", &labels(&[Some("_")]), None).as_deref(),
                Some("play:")
            );
            assert_eq!(
                objc_selector_for_swift_method("play", &labels(&[None]), None).as_deref(),
                Some("play:")
            );
        }

        #[test]
        fn single_labeled_param_base_with_label() {
            assert_eq!(
                objc_selector_for_swift_method("play", &labels(&[Some("song")]), None).as_deref(),
                Some("playWithSong:")
            );
        }

        #[test]
        fn multi_param_with_leading_underscore_base_label2_ellipsis() {
            assert_eq!(
                objc_selector_for_swift_method("play", &labels(&[Some("_"), Some("by")]), None)
                    .as_deref(),
                Some("play:by:")
            );
            assert_eq!(
                objc_selector_for_swift_method(
                    "tableView",
                    &labels(&[Some("_"), Some("didSelectRowAtIndexPath")]),
                    None
                )
                .as_deref(),
                Some("tableView:didSelectRowAtIndexPath:")
            );
        }

        #[test]
        fn multi_param_with_leading_explicit_label_base_with_first_rest() {
            assert_eq!(
                objc_selector_for_swift_method("play", &labels(&[Some("song"), Some("by")]), None)
                    .as_deref(),
                Some("playWithSong:by:")
            );
        }

        #[test]
        fn objc_custom_overrides_the_rule_literally() {
            assert_eq!(
                objc_selector_for_swift_method(
                    "whateverName",
                    &labels(&[Some("ignored")]),
                    Some("custom:")
                )
                .as_deref(),
                Some("custom:")
            );
        }

        #[test]
        fn returns_null_on_empty_base_name() {
            assert_eq!(objc_selector_for_swift_method("", &labels(&[]), None), None);
        }
    }

    mod objc_selector_for_swift_init {
        use super::*;

        #[test]
        fn init_no_parameters_init() {
            assert_eq!(
                objc_selector_for_swift_init(&labels(&[]), &names(&[]), None).as_deref(),
                Some("init")
            );
        }

        #[test]
        fn init_name_init_with_name() {
            assert_eq!(
                objc_selector_for_swift_init(&labels(&[Some("name")]), &names(&["name"]), None)
                    .as_deref(),
                Some("initWithName:")
            );
        }

        #[test]
        fn init_name_age_init_with_name_age() {
            assert_eq!(
                objc_selector_for_swift_init(
                    &labels(&[Some("name"), Some("age")]),
                    &names(&["name", "age"]),
                    None
                )
                .as_deref(),
                Some("initWithName:age:")
            );
        }

        #[test]
        fn init_underscore_name_uses_internal_name_init_with_name() {
            assert_eq!(
                objc_selector_for_swift_init(&labels(&[Some("_")]), &names(&["name"]), None)
                    .as_deref(),
                Some("initWithName:")
            );
        }

        #[test]
        fn objc_custom_override_on_init() {
            assert_eq!(
                objc_selector_for_swift_init(
                    &labels(&[Some("name")]),
                    &names(&["name"]),
                    Some("custom:")
                )
                .as_deref(),
                Some("custom:")
            );
        }
    }

    mod objc_accessors_for_swift_property {
        use super::*;

        #[test]
        fn getter_name_setter_set_name() {
            assert_eq!(
                objc_accessors_for_swift_property("name", None),
                accessors("name", "setName:")
            );
        }

        #[test]
        fn camel_case_set_capitalizes_first() {
            assert_eq!(
                objc_accessors_for_swift_property("isReady", None),
                accessors("isReady", "setIsReady:")
            );
        }

        #[test]
        fn explicit_objc_custom_overrides_getter_name() {
            assert_eq!(
                objc_accessors_for_swift_property("name", Some("displayName")),
                accessors("displayName", "setDisplayName:")
            );
        }
    }
}

mod objc_selector_to_swift_base_name_candidates_reverse_map {
    use super::*;

    #[test]
    fn bare_no_colon_selector_itself() {
        assert_eq!(swift_base_names_for_objc_selector("play"), names(&["play"]));
    }

    #[test]
    fn play_colon_play() {
        assert_eq!(
            swift_base_names_for_objc_selector("play:"),
            names(&["play"])
        );
    }

    #[test]
    fn play_with_song_play_with_song_and_play() {
        assert_eq!(
            sorted(swift_base_names_for_objc_selector("playWithSong:")),
            sorted(names(&["play", "playWithSong"]))
        );
    }

    #[test]
    fn cocoa_style_object_for_key_includes_object() {
        assert!(
            swift_base_names_for_objc_selector("objectForKey:").contains(&"object".to_string())
        );
    }

    #[test]
    fn cocoa_style_string_with_format_includes_string() {
        assert!(
            swift_base_names_for_objc_selector("stringWithFormat:").contains(&"string".to_string())
        );
    }

    #[test]
    fn cocoa_style_image_named_in_bundle_first_keyword_has_no_preposition_falls_through() {
        // First keyword is `imageNamed` -- no With/For/By in it, so candidates
        // is just the raw keyword. (`Named` is not in our preposition list --
        // keep it that way, otherwise we over-match on perfectly normal verbs.)
        assert_eq!(
            swift_base_names_for_objc_selector("imageNamed:inBundle:"),
            names(&["imageNamed"])
        );
    }

    #[test]
    fn play_by_play() {
        assert_eq!(
            swift_base_names_for_objc_selector("play:by:"),
            names(&["play"])
        );
    }

    #[test]
    fn play_with_song_by_play_with_song_and_play() {
        assert_eq!(
            sorted(swift_base_names_for_objc_selector("playWithSong:by:")),
            sorted(names(&["play", "playWithSong"]))
        );
    }

    #[test]
    fn init_with_name_includes_init() {
        assert!(swift_base_names_for_objc_selector("initWithName:").contains(&"init".to_string()));
    }

    #[test]
    fn init_with_name_age_includes_init() {
        assert!(
            swift_base_names_for_objc_selector("initWithName:age:").contains(&"init".to_string())
        );
    }

    #[test]
    fn set_name_includes_the_property_name_name() {
        assert!(swift_base_names_for_objc_selector("setName:").contains(&"name".to_string()));
    }

    #[test]
    fn table_view_did_select_row_at_index_path_table_view() {
        assert_eq!(
            swift_base_names_for_objc_selector("tableView:didSelectRowAtIndexPath:"),
            names(&["tableView"])
        );
    }
}

mod source_window_attribute_detection {
    use super::*;

    #[test]
    fn detects_literal_objc_custom() {
        assert_eq!(
            detect_explicit_objc_name("  @objc(custom:)\n  func foo() {}").as_deref(),
            Some("custom:")
        );
    }

    #[test]
    fn returns_null_for_plain_objc() {
        assert_eq!(detect_explicit_objc_name("@objc func foo() {}"), None);
    }

    #[test]
    fn returns_null_when_no_objc_at_all() {
        assert_eq!(detect_explicit_objc_name("public func foo() {}"), None);
    }

    #[test]
    fn is_objc_exposed_true_for_objc() {
        assert!(is_objc_exposed("@objc func foo() {}"));
    }

    #[test]
    fn is_objc_exposed_true_for_objc_custom() {
        assert!(is_objc_exposed("@objc(custom:) func foo() {}"));
    }

    #[test]
    fn is_objc_exposed_false_for_no_annotation() {
        assert!(!is_objc_exposed("public func foo() {}"));
    }

    #[test]
    fn nonobjc_opts_out_even_if_objc_also_present_inside_objc_members_class() {
        assert!(!is_objc_exposed("@nonobjc @objc func foo() {}"));
    }
}
