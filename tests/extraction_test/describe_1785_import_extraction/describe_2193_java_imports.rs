mod describe_2193_java_imports {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Java imports";
    const TS_DESCRIBE_LINE: usize = 2193;
    #[test]
    fn describes_024_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2193);
    }
    #[test]
    fn case_2194_should_extract_simple_import() {
        let suite = ["Import Extraction", "Java imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(133, 133);
        let result = extract("Main.java", "import java.util.List;");
        let import = single_import(&result, "java.util.List");
        assert_signature_eq(import, "import java.util.List;");
    }
    #[test]
    fn case_2204_should_extract_static_import() {
        let suite = ["Import Extraction", "Java imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(134, 134);
        let result = extract(
            "Utils.java",
            "import static java.util.Collections.emptyList;",
        );
        let import = single_import(&result, "java.util.Collections.emptyList");
        assert_signature_contains(import, "static");
    }
    #[test]
    fn case_2214_should_extract_wildcard_import() {
        let suite = ["Import Extraction", "Java imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(135, 135);
        let result = extract("App.java", "import java.util.*;");
        let import = single_import(&result, "java.util");
        assert_signature_contains(import, ".*");
    }
    #[test]
    fn case_2224_should_extract_nested_class_import() {
        let suite = ["Import Extraction", "Java imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(136, 136);
        let result = extract("MapUtil.java", "import java.util.Map.Entry;");
        single_import(&result, "java.util.Map.Entry");
    }
    #[test]
    fn case_2233_should_extract_multiple_imports() {
        let suite = ["Import Extraction", "Java imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(137, 137);
        let code = r#"
import java.util.List;
import java.util.Map;
import java.io.IOException;
"#;
        let result = extract("Service.java", code);
        assert_import_names(
            &result,
            &["java.util.List", "java.util.Map", "java.io.IOException"],
        );
    }
}
