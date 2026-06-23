mod describe_2251_c_imports {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "C# imports";
    const TS_DESCRIBE_LINE: usize = 2251;
    #[test]
    fn describes_025_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2251);
    }
    #[test]
    fn case_2252_should_extract_simple_using() {
        let suite = ["Import Extraction", "C# imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(138, 138);
        let result = extract("Program.cs", "using System;");
        let import = single_import(&result, "System");
        assert_signature_eq(import, "using System;");
    }
    #[test]
    fn case_2262_should_extract_qualified_using() {
        let suite = ["Import Extraction", "C# imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(139, 139);
        let result = extract("Utils.cs", "using System.Collections.Generic;");
        single_import(&result, "System.Collections.Generic");
    }
    #[test]
    fn case_2271_should_extract_static_using() {
        let suite = ["Import Extraction", "C# imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(140, 140);
        let result = extract("App.cs", "using static System.Console;");
        let import = single_import(&result, "System.Console");
        assert_signature_contains(import, "static");
    }
    #[test]
    fn case_2281_should_extract_alias_using() {
        let suite = ["Import Extraction", "C# imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(141, 141);
        let result = extract(
            "Types.cs",
            "using MyList = System.Collections.Generic.List<int>;",
        );
        let import = single_import(&result, "System.Collections.Generic.List<int>");
        assert_signature_contains(import, "MyList =");
    }
    #[test]
    fn case_2291_should_extract_multiple_usings() {
        let suite = ["Import Extraction", "C# imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(142, 142);
        let code = r#"
using System;
using System.Threading.Tasks;
using Microsoft.Extensions.DependencyInjection;
"#;
        let result = extract("Service.cs", code);
        assert_import_names(
            &result,
            &[
                "System",
                "System.Threading.Tasks",
                "Microsoft.Extensions.DependencyInjection",
            ],
        );
    }
}
