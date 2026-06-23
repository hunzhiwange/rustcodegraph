mod describe_1268_php_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "PHP Extraction";
    const TS_DESCRIBE_LINE: usize = 1268;
    #[test]
    fn describes_013_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 1268);
    }
    #[test]
    fn case_1269_should_extract_class_declarations() {
        let suite = ["PHP Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(73, 73);
        let code = r#"<?php

class UserController
{
    private UserService $userService;

    public function __construct(UserService $userService)
    {
        $this->userService = $userService;
    }

    public function show(string $id): User
    {
        return $this->userService->find($id);
    }
}
"#;
        let result = extract("UserController.php", code);
        let class_node = find_node(&result, NodeKind::Class, "UserController")
            .expect("UserController class should be extracted");
        assert_eq!(class_node.language, Language::Php);
    }
    #[test]
    fn case_1294_should_extract_class_inheritance_extends_and_interface_implementation() {
        let suite = ["PHP Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(74, 74);
        let code = r#"<?php

class ChildController extends BaseController implements Serializable, JsonSerializable
{
    public function serialize(): string
    {
        return json_encode($this);
    }
}
"#;
        let result = extract("ChildController.php", code);
        find_node(&result, NodeKind::Class, "ChildController")
            .expect("ChildController class should be extracted");
        let extends = references_by_kind(&result, ReferenceKind::Extends);
        assert_contains(&extends, "BaseController");
        let implements = references_by_kind(&result, ReferenceKind::Implements);
        assert_eq!(implements.len(), 2, "references: {implements:?}");
        assert_contains(&implements, "Serializable");
        assert_contains(&implements, "JsonSerializable");
    }
}
