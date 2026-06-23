//! Swift/Objective-C bridge resolver integration.
//!
//! This is the Rust port of `__tests__/swift-objc-bridge-resolver.test.ts`.

use std::collections::{HashMap, HashSet};

use rustcodegraph::resolution::frameworks::index::SWIFT_OBJC_BRIDGE_RESOLVER;
use rustcodegraph::resolution::types::{
    FrameworkResolver, ImportMapping, ResolutionContext, ResolvedBy, UnresolvedRef, language_name,
    now_ms,
};
use rustcodegraph::types::{Language, Node, NodeKind, ReferenceKind};

/// Lightweight ResolutionContext mock: implements only the methods the
/// bridge resolver actually calls. Anything else panics so a leaked call
/// surfaces loudly in tests.
struct MockResolutionContext {
    nodes: Vec<Node>,
    file_contents: HashMap<String, String>,
}

fn make_context(
    nodes: Vec<Node>,
    file_contents: impl IntoIterator<Item = (&'static str, &'static str)>,
) -> MockResolutionContext {
    MockResolutionContext {
        nodes,
        file_contents: file_contents
            .into_iter()
            .map(|(path, content)| (path.to_owned(), content.to_owned()))
            .collect(),
    }
}

impl ResolutionContext for MockResolutionContext {
    fn get_nodes_in_file(&mut self, file_path: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.file_path == file_path)
            .cloned()
            .collect()
    }

    fn get_nodes_by_name(&mut self, name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.name == name)
            .cloned()
            .collect()
    }

    fn get_nodes_by_qualified_name(&mut self, _qualified_name: &str) -> Vec<Node> {
        panic!("not used")
    }

    fn get_nodes_by_kind(&mut self, kind: NodeKind) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.kind == kind)
            .cloned()
            .collect()
    }

    fn file_exists(&mut self, file_path: &str) -> bool {
        self.nodes.iter().any(|node| node.file_path == file_path)
    }

    fn read_file(&mut self, file_path: &str) -> Option<String> {
        self.file_contents.get(file_path).cloned()
    }

    fn get_project_root(&self) -> String {
        "/test".to_owned()
    }

    fn get_all_files(&mut self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.nodes
            .iter()
            .filter_map(|node| {
                if seen.insert(node.file_path.clone()) {
                    Some(node.file_path.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    fn get_nodes_by_lower_name(&mut self, _lower_name: &str) -> Vec<Node> {
        panic!("not used")
    }

    fn get_import_mappings(&mut self, _file_path: &str, _language: Language) -> Vec<ImportMapping> {
        Vec::new()
    }
}

fn method(name: &str, language: Language, file_path: &str, start_line: u64) -> Node {
    Node {
        id: format!(
            "{}:{file_path}:{name}:{start_line}",
            language_name(language)
        ),
        kind: NodeKind::Method,
        name: name.to_owned(),
        qualified_name: format!("{file_path}::{name}"),
        file_path: file_path.to_owned(),
        language,
        start_line,
        end_line: start_line + 5,
        start_column: 0,
        end_column: 0,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: None,
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now_ms(),
    }
}

fn reference(name: &str, language: Language, file_path: &str) -> UnresolvedRef {
    UnresolvedRef {
        from_node_id: format!("caller:{file_path}"),
        reference_name: name.to_owned(),
        reference_kind: ReferenceKind::Calls,
        line: 1,
        column: 0,
        file_path: file_path.to_owned(),
        language,
        candidates: None,
    }
}

mod swift_objc_bridge_resolver_integration {
    use super::*;

    mod detect {
        use super::*;

        #[test]
        fn returns_true_when_both_swift_and_m_files_exist() {
            let mut ctx = make_context(
                vec![
                    method("foo", Language::Swift, "A.swift", 10),
                    method("bar", Language::ObjC, "B.m", 10),
                ],
                [],
            );

            assert!(SWIFT_OBJC_BRIDGE_RESOLVER.detect(&mut ctx));
        }

        #[test]
        fn returns_false_when_only_swift_files_exist() {
            let mut ctx = make_context(vec![method("foo", Language::Swift, "A.swift", 10)], []);

            assert!(!SWIFT_OBJC_BRIDGE_RESOLVER.detect(&mut ctx));
        }

        #[test]
        fn returns_true_when_swift_and_mm_exist_objc_plus_plus() {
            let mut ctx = make_context(
                vec![
                    method("foo", Language::Swift, "A.swift", 10),
                    method("bar", Language::ObjC, "B.mm", 10),
                ],
                [],
            );

            assert!(SWIFT_OBJC_BRIDGE_RESOLVER.detect(&mut ctx));
        }
    }

    mod claims_reference {
        use super::*;

        #[test]
        fn claims_selector_shape_names_contain_colon() {
            assert!(SWIFT_OBJC_BRIDGE_RESOLVER.claims_reference("fooWithBar:"));
            assert!(
                SWIFT_OBJC_BRIDGE_RESOLVER.claims_reference("tableView:didSelectRowAtIndexPath:")
            );
            assert!(SWIFT_OBJC_BRIDGE_RESOLVER.claims_reference("setName:"));
        }

        #[test]
        fn does_not_claim_bare_names_handled_by_normal_name_matcher() {
            assert!(!SWIFT_OBJC_BRIDGE_RESOLVER.claims_reference("foo"));
            assert!(!SWIFT_OBJC_BRIDGE_RESOLVER.claims_reference("init"));
        }
    }

    mod resolve_swift_to_objc_direction {
        use super::*;

        #[test]
        fn resolves_swift_call_to_cocoa_style_objc_method_fetch_entry_to_fetch_entry_for_key() {
            // Swift writes `cache.fetchEntry(forKey: "x")` -> ref name `fetchEntry`.
            // ObjC method is `fetchEntryForKey:` (preposition-prefix shape).
            // `fetchEntry` is project-specific (not in the generic-names blocklist
            // that filters init/count/description/etc. to avoid Cocoa noise).
            let objc_target = method("fetchEntryForKey:", Language::ObjC, "Cache.m", 10);
            let mut ctx = make_context(vec![objc_target.clone()], []);

            let result = SWIFT_OBJC_BRIDGE_RESOLVER.resolve(
                &reference("fetchEntry", Language::Swift, "Caller.swift"),
                &mut ctx,
            );

            let result = result.expect("Swift call should resolve to ObjC target");
            assert_eq!(result.target_node_id, objc_target.id);
            assert_eq!(result.resolved_by, ResolvedBy::Framework);
            assert_eq!(result.confidence, 0.6);
        }

        #[test]
        fn does_not_bridge_generic_cocoa_names_like_init_or_description() {
            // Bridging Swift `init()` calls to arbitrary ObjC `init*:` methods is
            // noise: every NSObject subclass has them. The regular name-matcher
            // handles `init` on its own.
            let objc_init = method("initWithFrame:", Language::ObjC, "View.m", 10);
            let mut ctx = make_context(vec![objc_init], []);

            let result = SWIFT_OBJC_BRIDGE_RESOLVER.resolve(
                &reference("init", Language::Swift, "Caller.swift"),
                &mut ctx,
            );

            assert!(result.is_none());
        }

        #[test]
        fn resolves_bridged_with_form_swift_play_song_to_objc_play_with_song() {
            let objc_target = method("playWithSong:", Language::ObjC, "Player.m", 10);
            let mut ctx = make_context(vec![objc_target.clone()], []);

            let result = SWIFT_OBJC_BRIDGE_RESOLVER.resolve(
                &reference("play", Language::Swift, "Caller.swift"),
                &mut ctx,
            );

            assert_eq!(
                result.map(|resolved| resolved.target_node_id),
                Some(objc_target.id)
            );
        }

        #[test]
        fn returns_null_when_no_matching_objc_method_exists() {
            let mut ctx = make_context(
                vec![method("unrelated:thing:", Language::ObjC, "X.m", 10)],
                [],
            );

            let result = SWIFT_OBJC_BRIDGE_RESOLVER.resolve(
                &reference("completelyDifferent", Language::Swift, "Caller.swift"),
                &mut ctx,
            );

            assert!(result.is_none());
        }
    }

    mod resolve_objc_to_swift_direction {
        use super::*;

        #[test]
        fn resolves_objc_selector_to_objc_exposed_swift_method_exporter_form() {
            // Swift @objc export of `func animate(xAxisDuration:, yAxisDuration:)`
            // produces ObjC selector `animateWithXAxisDuration:yAxisDuration:`
            // (always "With" insertion on first explicit label).
            let swift_target = method("animate", Language::Swift, "Chart.swift", 10);
            let mut ctx = make_context(
                vec![swift_target.clone()],
                [(
                    "Chart.swift",
                    "\n\n\n\n\n\n\n\n@objc open func animate(xAxisDuration: Double, yAxisDuration: Double) {}\n",
                )],
            );

            let result = SWIFT_OBJC_BRIDGE_RESOLVER.resolve(
                &reference(
                    "animateWithXAxisDuration:yAxisDuration:",
                    Language::ObjC,
                    "Caller.m",
                ),
                &mut ctx,
            );

            let result = result.expect("ObjC selector should resolve to Swift target");
            assert_eq!(result.target_node_id, swift_target.id);
            assert_eq!(result.resolved_by, ResolvedBy::Framework);
        }

        #[test]
        fn does_not_resolve_if_the_swift_method_is_not_objc_exposed() {
            let swift_target = method("animate", Language::Swift, "Chart.swift", 10);
            let mut ctx = make_context(
                vec![swift_target],
                [(
                    "Chart.swift",
                    // Plain `func` without @objc: bridge correctly skips it
                    "\n\n\n\n\n\n\n\nfunc animate(xAxisDuration: Double, yAxisDuration: Double) {}\n",
                )],
            );

            let result = SWIFT_OBJC_BRIDGE_RESOLVER.resolve(
                &reference(
                    "animateWithXAxisDuration:yAxisDuration:",
                    Language::ObjC,
                    "Caller.m",
                ),
                &mut ctx,
            );

            assert!(result.is_none());
        }

        #[test]
        fn resolves_init_selectors_to_swift_init() {
            let swift_target = method("init", Language::Swift, "MyClass.swift", 10);
            let mut ctx = make_context(
                vec![swift_target.clone()],
                [(
                    "MyClass.swift",
                    "\n\n\n\n\n\n\n\n@objc init(name: String, age: Int) {}\n",
                )],
            );

            let result = SWIFT_OBJC_BRIDGE_RESOLVER.resolve(
                &reference("initWithName:age:", Language::ObjC, "Caller.m"),
                &mut ctx,
            );

            assert_eq!(
                result.map(|resolved| resolved.target_node_id),
                Some(swift_target.id)
            );
        }

        #[test]
        fn returns_null_for_selectors_with_no_derivable_swift_candidates_that_exist() {
            let mut ctx = make_context(vec![], []);

            let result = SWIFT_OBJC_BRIDGE_RESOLVER.resolve(
                &reference("someUnknownThing:", Language::ObjC, "Caller.m"),
                &mut ctx,
            );

            assert!(result.is_none());
        }
    }
}
