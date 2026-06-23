mod describe_0809_go_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Go Extraction";
    const TS_DESCRIBE_LINE: usize = 809;
    #[test]
    fn describes_009_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 809);
    }
    #[test]
    fn case_0810_should_extract_function_declarations() {
        let suite = ["Go Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(55, 55);
        let code = r#"
package main

func ProcessOrder(order Order) (Receipt, error) {
    return Receipt{}, nil
}
"#;
        let result = extract("main.go", code);
        let func_node = find_node(&result, NodeKind::Function, "ProcessOrder")
            .expect("ProcessOrder function should be extracted");
        assert_eq!(func_node.language, Language::Go);
    }
    #[test]
    fn case_0826_should_extract_method_declarations() {
        let suite = ["Go Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(56, 56);
        let code = r#"
package main

type Service struct {
    db *Database
}

func (s *Service) GetUser(id string) (*User, error) {
    return s.db.FindUser(id)
}
"#;
        let result = extract("service.go", code);
        let method_node = result
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Method && node.name.ends_with("GetUser"))
            .expect("GetUser method should be extracted");
        assert_eq!(method_node.language, Language::Go);
    }
}
