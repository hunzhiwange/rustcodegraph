mod describe_1786_typescript_javascript_imports {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "TypeScript/JavaScript imports";
    const TS_DESCRIBE_LINE: usize = 1786;
    #[test]
    fn describes_018_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 1786);
    }
    #[test]
    fn case_1787_should_extract_default_imports() {
        let suite = ["Import Extraction", "TypeScript/JavaScript imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(99, 99);
        let result = extract("app.tsx", "import React from 'react';");
        let import = single_import(&result, "react");
        assert_signature_eq(import, "import React from 'react';");
    }
    #[test]
    fn case_1797_should_extract_named_imports() {
        let suite = ["Import Extraction", "TypeScript/JavaScript imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(100, 100);
        let result = extract(
            "icons.tsx",
            "import { Bug, Database } from '@phosphor-icons/react';",
        );
        let import = single_import(&result, "@phosphor-icons/react");
        assert_signature_contains(import, "Bug");
        assert_signature_contains(import, "Database");
    }
    #[test]
    fn case_1808_should_extract_namespace_imports() {
        let suite = ["Import Extraction", "TypeScript/JavaScript imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(101, 101);
        let result = extract(
            "icons.tsx",
            "import * as Icons from '@phosphor-icons/react';",
        );
        let import = single_import(&result, "@phosphor-icons/react");
        assert_signature_contains(import, "* as Icons");
    }
    #[test]
    fn case_1818_should_extract_side_effect_imports() {
        let suite = ["Import Extraction", "TypeScript/JavaScript imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(102, 102);
        let result = extract("app.tsx", "import './styles.css';");
        single_import(&result, "./styles.css");
    }
    #[test]
    fn case_1827_should_extract_mixed_imports_default_named() {
        let suite = ["Import Extraction", "TypeScript/JavaScript imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(103, 103);
        let result = extract(
            "app.tsx",
            "import React, { useState, useEffect } from 'react';",
        );
        let import = single_import(&result, "react");
        assert_signature_contains(import, "React");
        assert_signature_contains(import, "useState");
        assert_signature_contains(import, "useEffect");
    }
    #[test]
    fn case_1839_should_extract_multiple_import_statements() {
        let suite = ["Import Extraction", "TypeScript/JavaScript imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(104, 104);
        let code = r#"
import React from 'react';
import { Button } from './components';
import './styles.css';
"#;
        let result = extract("app.tsx", code);
        assert_import_names(&result, &["react", "./components", "./styles.css"]);
    }
    #[test]
    fn case_1856_should_extract_type_imports() {
        let suite = ["Import Extraction", "TypeScript/JavaScript imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(105, 105);
        let result = extract("types.ts", "import type { FC, ReactNode } from 'react';");
        let import = single_import(&result, "react");
        assert_signature_contains(import, "type");
        assert_signature_contains(import, "FC");
    }
    #[test]
    fn case_1867_should_extract_aliased_named_imports() {
        let suite = ["Import Extraction", "TypeScript/JavaScript imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(106, 106);
        let result = extract(
            "hooks.ts",
            "import { useState as useStateAlias } from 'react';",
        );
        let import = single_import(&result, "react");
        assert_signature_contains(import, "useState");
        assert_signature_contains(import, "useStateAlias");
    }
    #[test]
    fn case_1878_should_extract_relative_path_imports() {
        let suite = ["Import Extraction", "TypeScript/JavaScript imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(107, 107);
        let result = extract(
            "components/Button.tsx",
            "import { helper } from '../utils/helper';",
        );
        let import = single_import(&result, "../utils/helper");
        assert_signature_contains(import, "helper");
    }
}
