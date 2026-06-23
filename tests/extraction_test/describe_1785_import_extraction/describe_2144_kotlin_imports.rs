mod describe_2144_kotlin_imports {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Kotlin imports";
    const TS_DESCRIBE_LINE: usize = 2144;
    #[test]
    fn describes_023_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2144);
    }
    #[test]
    fn case_2145_should_extract_simple_import() {
        let suite = ["Import Extraction", "Kotlin imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(129, 129);
        let result = extract("Main.kt", "import java.io.IOException");
        let import = single_import(&result, "java.io.IOException");
        assert_signature_eq(import, "import java.io.IOException");
    }
    #[test]
    fn case_2155_should_extract_aliased_import() {
        let suite = ["Import Extraction", "Kotlin imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(130, 130);
        let result = extract(
            "Utils.kt",
            "import okhttp3.Request.Builder as RequestBuilder",
        );
        let import = single_import(&result, "okhttp3.Request.Builder");
        assert_signature_contains(import, "as RequestBuilder");
    }
    #[test]
    fn case_2165_should_extract_wildcard_import() {
        let suite = ["Import Extraction", "Kotlin imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(131, 131);
        let result = extract("Time.kt", "import java.util.concurrent.TimeUnit.*");
        let import = single_import(&result, "java.util.concurrent.TimeUnit");
        assert_signature_contains(import, ".*");
    }
    #[test]
    fn case_2175_should_extract_multiple_imports() {
        let suite = ["Import Extraction", "Kotlin imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(132, 132);
        let code = r#"
import java.io.IOException
import kotlin.test.assertFailsWith
import okhttp3.OkHttpClient
"#;
        let result = extract("Test.kt", code);
        assert_import_names(
            &result,
            &[
                "java.io.IOException",
                "kotlin.test.assertFailsWith",
                "okhttp3.OkHttpClient",
            ],
        );
    }
}
