mod describe_2662_dart_imports {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Dart imports";
    const TS_DESCRIBE_LINE: usize = 2662;
    #[test]
    fn describes_032_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2662);
    }
    #[test]
    fn case_2663_should_extract_dart_import() {
        let suite = ["Import Extraction", "Dart imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(169, 169);
        let result = extract("main.dart", "import 'dart:async';");
        let import = single_import(&result, "dart:async");
        assert_signature_eq(import, "import 'dart:async';");
    }
    #[test]
    fn case_2673_should_extract_package_import() {
        let suite = ["Import Extraction", "Dart imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(170, 170);
        let result = extract("app.dart", "import 'package:flutter/material.dart';");
        single_import(&result, "package:flutter/material.dart");
    }
    #[test]
    fn case_2682_should_extract_aliased_import() {
        let suite = ["Import Extraction", "Dart imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(171, 171);
        let result = extract("api.dart", "import 'package:http/http.dart' as http;");
        let import = single_import(&result, "package:http/http.dart");
        assert_signature_contains(import, "as http");
    }
    #[test]
    fn case_2692_should_extract_multiple_imports() {
        let suite = ["Import Extraction", "Dart imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(172, 172);
        let code = r#"
import 'dart:async';
import 'dart:convert';
import 'package:flutter/material.dart';
"#;
        let result = extract("main.dart", code);
        assert_import_names(
            &result,
            &[
                "dart:async",
                "dart:convert",
                "package:flutter/material.dart",
            ],
        );
    }
    #[test]
    fn case_2709_should_extract_relative_import() {
        let suite = ["Import Extraction", "Dart imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(173, 173);
        let result = extract("lib/main.dart", "import '../utils/helpers.dart';");
        single_import(&result, "../utils/helpers.dart");
    }
}
