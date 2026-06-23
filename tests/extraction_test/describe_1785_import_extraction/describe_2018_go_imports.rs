mod describe_2018_go_imports {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Go imports";
    const TS_DESCRIBE_LINE: usize = 2018;
    #[test]
    fn describes_021_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2018);
    }
    #[test]
    fn case_2019_should_extract_single_import() {
        let suite = ["Import Extraction", "Go imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(120, 120);
        let code = r#"
package main

import "fmt"
"#;
        let result = extract("main.go", code);
        single_import(&result, "fmt");
    }
    #[test]
    fn case_2032_should_extract_grouped_imports() {
        let suite = ["Import Extraction", "Go imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(121, 121);
        let code = r#"
package main

import (
    "fmt"
    "os"
    "encoding/json"
)
"#;
        let result = extract("main.go", code);
        assert_import_names(&result, &["fmt", "os", "encoding/json"]);
    }
    #[test]
    fn case_2053_should_extract_aliased_import() {
        let suite = ["Import Extraction", "Go imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(122, 122);
        let code = r#"
package main

import f "fmt"
"#;
        let result = extract("main.go", code);
        let import = single_import(&result, "fmt");
        assert_signature_contains(import, "f");
    }
    #[test]
    fn case_2067_should_extract_dot_import() {
        let suite = ["Import Extraction", "Go imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(123, 123);
        let code = r#"
package main

import . "math"
"#;
        let result = extract("main.go", code);
        let import = single_import(&result, "math");
        assert_signature_contains(import, ".");
    }
    #[test]
    fn case_2081_should_extract_blank_import() {
        let suite = ["Import Extraction", "Go imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(124, 124);
        let code = r#"
package main

import _ "github.com/go-sql-driver/mysql"
"#;
        let result = extract("main.go", code);
        let import = single_import(&result, "github.com/go-sql-driver/mysql");
        assert_signature_contains(import, "_");
    }
}
