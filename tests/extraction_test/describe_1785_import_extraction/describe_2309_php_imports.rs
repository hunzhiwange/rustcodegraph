mod describe_2309_php_imports {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "PHP imports";
    const TS_DESCRIBE_LINE: usize = 2309;
    #[test]
    fn describes_026_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2309);
    }
    #[test]
    fn case_2310_should_extract_simple_use() {
        let suite = ["Import Extraction", "PHP imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(143, 143);
        let result = extract("Test.php", "<?php use PHPUnit\\Framework\\TestCase;");
        single_import(&result, "PHPUnit\\Framework\\TestCase");
    }
    #[test]
    fn case_2319_should_extract_aliased_use() {
        let suite = ["Import Extraction", "PHP imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(144, 144);
        let result = extract("Test.php", "<?php use Mockery as m;");
        let import = single_import(&result, "Mockery");
        assert_signature_contains(import, "as m");
    }
    #[test]
    fn case_2329_should_extract_function_use() {
        let suite = ["Import Extraction", "PHP imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(145, 145);
        let result = extract(
            "helpers.php",
            "<?php use function Illuminate\\Support\\env;",
        );
        let import = single_import(&result, "Illuminate\\Support\\env");
        assert_signature_contains(import, "function");
    }
    #[test]
    fn case_2339_should_extract_grouped_use() {
        let suite = ["Import Extraction", "PHP imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(146, 146);
        let result = extract(
            "Models.php",
            "<?php use Illuminate\\Database\\{Model, Builder};",
        );
        assert_import_names(
            &result,
            &[
                "Illuminate\\Database\\Model",
                "Illuminate\\Database\\Builder",
            ],
        );
    }
    #[test]
    fn case_2351_should_extract_multiple_uses() {
        let suite = ["Import Extraction", "PHP imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(147, 147);
        let code = r#"<?php
use Illuminate\Support\Collection;
use Illuminate\Support\Str;
use Closure;
"#;
        let result = extract("Service.php", code);
        assert_import_names(
            &result,
            &[
                "Illuminate\\Support\\Collection",
                "Illuminate\\Support\\Str",
                "Closure",
            ],
        );
    }
    #[test]
    fn case_2368_should_extract_include_require_once_static_paths_as_imports_660() {
        let suite = ["Import Extraction", "PHP imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(148, 148);
        let code = r#"<?php
require_once("lib.php");
include 'other.php';
require 'r.php';
include_once("io.php");
"#;
        let result = extract("page.php", code);
        assert_import_names(&result, &["lib.php", "other.php", "r.php", "io.php"]);
    }
    #[test]
    fn case_2383_should_skip_dynamic_include_require_with_no_static_path_660() {
        let suite = ["Import Extraction", "PHP imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(149, 149);
        let code = r#"<?php
require_once(__DIR__ . '/dyn.php');
include $file;
include "tpl/{$name}.php";
"#;
        let result = extract("page.php", code);
        assert_no_imports(&result);
    }
    #[test]
    fn case_2394_should_extract_include_alongside_namespace_use_without_interference_66() {
        let suite = ["Import Extraction", "PHP imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(150, 150);
        let code = r#"<?php
use App\Service\Mailer;
require_once("bootstrap.php");
"#;
        let result = extract("page.php", code);
        assert_import_names(&result, &["App\\Service\\Mailer", "bootstrap.php"]);
    }
}
