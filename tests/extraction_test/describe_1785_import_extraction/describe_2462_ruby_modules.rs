mod describe_2462_ruby_modules {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Ruby modules";
    const TS_DESCRIBE_LINE: usize = 2462;
    #[test]
    fn describes_028_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2462);
    }
    #[test]
    fn case_2463_should_extract_module_as_module_node_with_containment() {
        let suite = ["Import Extraction", "Ruby modules"];
        assert_eq!(suite.len(), 2);
        assert_eq!(156, 156);
        let code = r#"
module CachedCounting
  def self.disable
    @enabled = false
  end

  def perform_increment!(key, count)
    write_cache!(key, count)
  end
end
"#;
        let result = extract("concerns/cached_counting.rb", code);
        let module_node = find_node(&result, NodeKind::Module, "CachedCounting")
            .expect("CachedCounting module should be extracted");
        assert!(module_node.qualified_name.ends_with("CachedCounting"));

        let disable_method = find_node(&result, NodeKind::Method, "disable")
            .expect("disable method should be extracted");
        assert!(disable_method
            .qualified_name
            .ends_with("CachedCounting::disable"));
        let increment_method = find_node(&result, NodeKind::Method, "perform_increment!")
            .expect("perform_increment! method should be extracted");
        assert!(increment_method
            .qualified_name
            .ends_with("CachedCounting::perform_increment!"));

        let contains = result
            .edges
            .iter()
            .filter(|edge| edge.source == module_node.id && edge.kind == EdgeKind::Contains)
            .count();
        assert!(contains >= 2, "edges: {:?}", result.edges);
    }
    #[test]
    fn case_2495_should_handle_nested_modules_with_classes() {
        let suite = ["Import Extraction", "Ruby modules"];
        assert_eq!(suite.len(), 2);
        assert_eq!(157, 157);
        let code = r#"
module Discourse
  module Auth
    class AuthProvider
      def authenticate(params)
        validate(params)
      end
    end
  end
end
"#;
        let result = extract("lib/auth.rb", code);
        find_node(&result, NodeKind::Module, "Discourse")
            .expect("Discourse module should be extracted");
        let auth_module =
            find_node(&result, NodeKind::Module, "Auth").expect("Auth module should exist");
        assert!(auth_module.qualified_name.ends_with("Discourse::Auth"));
        let auth_provider = find_node(&result, NodeKind::Class, "AuthProvider")
            .expect("AuthProvider should be extracted");
        assert!(auth_provider
            .qualified_name
            .ends_with("Discourse::Auth::AuthProvider"));
        let auth_method = find_node(&result, NodeKind::Method, "authenticate")
            .expect("authenticate method should be extracted");
        assert!(auth_method
            .qualified_name
            .ends_with("Discourse::Auth::AuthProvider::authenticate"));
    }
}
