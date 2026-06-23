//! Rust port of `__tests__/is-test-file.test.ts`.

use rustcodegraph::search::query_utils::is_test_file;

mod is_test_file_tests {
    use super::*;

    #[test]
    fn flags_kotlin_test_files_and_source_sets() {
        assert!(is_test_file(
            "okhttp/src/jvmTest/kotlin/okhttp3/CallTest.kt"
        ));
        assert!(is_test_file(
            "okhttp/src/commonTest/kotlin/okhttp3/CompressionInterceptorTest.kt"
        ));
        assert!(is_test_file(
            "app/src/androidTest/java/com/example/FooTest.kt"
        ));
        assert!(is_test_file("module/src/integrationTest/kotlin/BarSpec.kt"));
    }

    #[test]
    fn flags_swift_test_files() {
        assert!(is_test_file("Tests/SessionTests.swift"));
        assert!(is_test_file("Sources/FooTest.swift"));
    }

    #[test]
    fn still_flags_the_previously_supported_conventions() {
        assert!(is_test_file("foo/test_bar.py"));
        assert!(is_test_file("pkg/bar_test.go"));
        assert!(is_test_file("src/foo.test.ts"));
        assert!(is_test_file("src/foo.spec.ts"));
        assert!(is_test_file("com/example/FooTest.java"));
        assert!(is_test_file("com/example/FooTestCase.java"));
        assert!(is_test_file("project/__tests__/foo.ts"));
        assert!(is_test_file("project/tests/foo.rb"));
    }

    #[test]
    fn does_not_flag_production_files_that_merely_contain_test_lowercase() {
        assert!(!is_test_file("src/latest/loader.kt"));
        assert!(!is_test_file("lib/manifest.kt"));
        assert!(!is_test_file(
            "okhttp/src/jvmMain/kotlin/okhttp3/internal/connection/RealCall.kt"
        ));
        assert!(!is_test_file("src/contestEntry.ts"));
        assert!(!is_test_file("pkg/greatest.go"));
    }

    #[test]
    fn does_not_flag_ordinary_production_source() {
        assert!(!is_test_file("src/flask/app.py"));
        assert!(!is_test_file(
            "src/vs/workbench/api/common/extensionHostMain.ts"
        ));
        assert!(!is_test_file(
            "okhttp/src/commonJvmAndroid/kotlin/okhttp3/OkHttpClient.kt"
        ));
    }
}
