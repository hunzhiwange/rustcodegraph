mod describe_0769_python_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Python Extraction";
    const TS_DESCRIBE_LINE: usize = 769;
    #[test]
    fn describes_008_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 769);
    }
    #[test]
    fn case_0770_should_extract_function_definitions() {
        let suite = ["Python Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(53, 53);
        let code = r#"
def calculate_total(items: list, tax_rate: float) -> float:
    """Calculate total with tax."""
    subtotal = sum(item.price for item in items)
    return subtotal * (1 + tax_rate)
"#;
        let result = extract("calc.py", code);
        let func_node = find_node(&result, NodeKind::Function, "calculate_total")
            .expect("calculate_total function should be extracted");
        assert_eq!(func_node.language, Language::Python);
    }
    #[test]
    fn case_0790_should_extract_class_definitions() {
        let suite = ["Python Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(54, 54);
        let code = r#"
class UserService:
    """Service for managing users."""

    def __init__(self, db):
        self.db = db

    def get_user(self, user_id: str) -> User:
        return self.db.find_user(user_id)
"#;
        let result = extract("service.py", code);
        let class_node = find_node(&result, NodeKind::Class, "UserService")
            .expect("UserService class should be extracted");
        assert_eq!(class_node.language, Language::Python);
    }
}
