mod describe_2526_php_return_type_capture_608 {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "PHP return type capture (#608)";
    const TS_DESCRIBE_LINE: usize = 2526;
    #[test]
    fn describes_029_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2526);
    }
    #[test]
    fn case_2527_captures_self_static_factory_returns_as_the_self_marker_primitives_as_() {
        let suite = ["Import Extraction", "PHP return type capture (#608)"];
        assert_eq!(suite.len(), 2);
        assert_eq!(158, 158);
        let code = r#"<?php
class ApiClient {
    public static function for(string $c): self { return new self; }
    public static function make(): static { return new static; }
    public function send(array $p): array { return []; }
}"#;
        let result = extract("ApiClient.php", code);
        assert_return_type(&result, NodeKind::Method, "for", Some("self"));
        assert_return_type(&result, NodeKind::Method, "make", Some("self"));
        assert_return_type(&result, NodeKind::Method, "send", None);
    }
    #[test]
    fn case_2541_captures_a_concrete_return_type_as_its_short_class_name() {
        let suite = ["Import Extraction", "PHP return type capture (#608)"];
        assert_eq!(suite.len(), 2);
        assert_eq!(159, 159);
        let code = r#"<?php
namespace App;
class WidgetFactory { public static function make(): Widget { return new Widget(); } }"#;
        let result = extract("WidgetFactory.php", code);
        assert_return_type(&result, NodeKind::Method, "make", Some("Widget"));
    }
}
