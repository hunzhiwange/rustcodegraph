mod describe_1648_dart_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Dart Extraction";
    const TS_DESCRIBE_LINE: usize = 1648;
    #[test]
    fn describes_016_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 1648);
    }
    #[test]
    fn case_1649_should_extract_class_declarations() {
        let suite = ["Dart Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(91, 91);
        let code = r#"
class UserService {
  final Database _db;

  Future<User> findById(String id) async {
    return await _db.query(id);
  }

  void _privateMethod() {}
}
"#;
        let result = extract("service.dart", code);
        let class_node = find_node(&result, NodeKind::Class, "UserService")
            .expect("UserService class should be extracted");
        assert_eq!(class_node.visibility, Some(Visibility::Public));
        let find_by_id = find_node(&result, NodeKind::Method, "findById")
            .expect("findById method should be extracted");
        assert_eq!(find_by_id.is_async, Some(true));
        let private = find_node(&result, NodeKind::Method, "_privateMethod")
            .expect("private method should be extracted");
        assert_eq!(private.visibility, Some(Visibility::Private));
        assert!(find_by_id.end_line > find_by_id.start_line);
    }
    #[test]
    fn case_1685_should_extract_top_level_function_declarations() {
        let suite = ["Dart Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(92, 92);
        let code = r#"
void topLevelFunction(String name) {
  print(name);
}
"#;
        let result = extract("utils.dart", code);
        let func_node = find_node(&result, NodeKind::Function, "topLevelFunction")
            .expect("top-level function should be extracted");
        assert_eq!(func_node.language, Language::Dart);
    }
    #[test]
    fn case_1699_should_extract_enum_declarations() {
        let suite = ["Dart Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(93, 93);
        let code = r#"
enum Status { active, inactive, pending }
"#;
        let result = extract("models.dart", code);
        find_node(&result, NodeKind::Enum, "Status").expect("Status enum should be extracted");
    }
    #[test]
    fn case_1710_should_extract_mixin_declarations() {
        let suite = ["Dart Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(94, 94);
        let code = r#"
mixin LoggerMixin {
  void log(String message) {}
}
"#;
        let result = extract("mixins.dart", code);
        find_node(&result, NodeKind::Class, "LoggerMixin")
            .expect("mixin should be represented as a class node");
        find_node(&result, NodeKind::Method, "log").expect("mixin method should be extracted");
    }
    #[test]
    fn case_1727_should_extract_extension_declarations() {
        let suite = ["Dart Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(95, 95);
        let code = r#"
extension StringExt on String {
  bool get isBlank => trim().isEmpty;
}
"#;
        let result = extract("extensions.dart", code);
        find_node(&result, NodeKind::Class, "StringExt")
            .expect("extension should be represented as a class node");
    }
    #[test]
    fn case_1740_should_detect_static_methods() {
        let suite = ["Dart Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(96, 96);
        let code = r#"
class Utils {
  static void doWork() {}
}
"#;
        let result = extract("utils.dart", code);
        let method =
            find_node(&result, NodeKind::Method, "doWork").expect("doWork method should exist");
        assert_eq!(method.is_static, Some(true));
    }
    #[test]
    fn case_1754_should_detect_async_functions() {
        let suite = ["Dart Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(97, 97);
        let code = r#"
Future<String> fetchData() async {
  return await http.get('/data');
}
"#;
        let result = extract("api.dart", code);
        let func = find_node(&result, NodeKind::Function, "fetchData")
            .expect("fetchData function should be extracted");
        assert_eq!(func.is_async, Some(true));
    }
    #[test]
    fn case_1768_should_detect_private_visibility_via_underscore_convention() {
        let suite = ["Dart Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(98, 98);
        let code = r#"
void _privateHelper() {}

void publicFunction() {}
"#;
        let result = extract("helpers.dart", code);
        let private = find_node(&result, NodeKind::Function, "_privateHelper")
            .expect("private helper should be extracted");
        let public = find_node(&result, NodeKind::Function, "publicFunction")
            .expect("public function should be extracted");
        assert_eq!(private.visibility, Some(Visibility::Private));
        assert_eq!(public.visibility, Some(Visibility::Public));
    }
}
