mod describe_2550_c_c_return_type_capture_645 {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "C/C++ return type capture (#645)";
    const TS_DESCRIBE_LINE: usize = 2550;
    #[test]
    fn describes_030_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2550);
    }
    #[test]
    fn case_2551_captures_the_normalized_return_type_of_a_c_method_function() {
        let suite = ["Import Extraction", "C/C++ return type capture (#645)"];
        assert_eq!(suite.len(), 2);
        assert_eq!(160, 160);
        let code = r#"
struct Widget { void draw(); };
class Factory { public: static Widget create(); };
Widget Factory::create() { return Widget(); }
void doNothing() {}
"#;
        let result = extract("f.cpp", code);
        let create = result
            .nodes
            .iter()
            .find(|node| {
                node.name == "create"
                    && (node.kind == NodeKind::Method || node.kind == NodeKind::Function)
            })
            .expect("create should be extracted");
        assert_eq!(create.return_type.as_deref(), Some("Widget"));
        assert_return_type(&result, NodeKind::Function, "doNothing", None);
    }
    #[test]
    fn case_2572_unwraps_a_smart_pointer_return_type_to_its_pointee() {
        let suite = ["Import Extraction", "C/C++ return type capture (#645)"];
        assert_eq!(suite.len(), 2);
        assert_eq!(161, 161);
        let code = r#"
#include <memory>
struct Widget {};
std::unique_ptr<Widget> makeWidget() { return nullptr; }
"#;
        let result = extract("f.cpp", code);
        assert_return_type(&result, NodeKind::Function, "makeWidget", Some("Widget"));
    }
}
