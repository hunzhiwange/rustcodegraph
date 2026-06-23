use super::*;

mod framework_resolver_extract_interface {
    use super::*;

    // describe('FrameworkResolver.extract interface')
    // it('extract() returns { nodes, references }')
    #[test]
    fn extract_returns_nodes_references() {
        let resolver = NamedResolver {
            name: "fake",
            languages: Some(&[Language::Python]),
        };
        let result = resolver.extract("foo.py", "");
        assert!(result.nodes.is_empty());
        assert!(result.references.is_empty());
    }
}

mod get_applicable_frameworks_tests {
    use super::*;

    // describe('getApplicableFrameworks')
    // it('filters by language')
    #[test]
    fn filters_by_language() {
        let py_fw: ResolverRef = Arc::new(NamedResolver {
            name: "py",
            languages: Some(&[Language::Python]),
        });
        let js_fw: ResolverRef = Arc::new(NamedResolver {
            name: "js",
            languages: Some(&[Language::JavaScript, Language::TypeScript]),
        });
        let any_fw: ResolverRef = Arc::new(NamedResolver {
            name: "any",
            languages: None,
        });
        let result = get_applicable_frameworks(&[py_fw, js_fw, any_fw], Language::Python);
        assert_eq!(
            result
                .iter()
                .map(|resolver| resolver.name().to_string())
                .collect::<Vec<_>>(),
            vec!["py", "any"]
        );
    }

    // it('returns anyFw-only when language has no matches')
    #[test]
    fn returns_any_fw_only_when_language_has_no_matches() {
        let py_fw: ResolverRef = Arc::new(NamedResolver {
            name: "py",
            languages: Some(&[Language::Python]),
        });
        let js_fw: ResolverRef = Arc::new(NamedResolver {
            name: "js",
            languages: Some(&[Language::JavaScript, Language::TypeScript]),
        });
        let any_fw: ResolverRef = Arc::new(NamedResolver {
            name: "any",
            languages: None,
        });
        let result = get_applicable_frameworks(&[py_fw, js_fw, any_fw], Language::Rust);
        assert_eq!(
            result
                .iter()
                .map(|resolver| resolver.name().to_string())
                .collect::<Vec<_>>(),
            vec!["any"]
        );
    }
}
