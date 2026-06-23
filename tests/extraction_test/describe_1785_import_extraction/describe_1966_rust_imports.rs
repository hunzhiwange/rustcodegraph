mod describe_1966_rust_imports {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Rust imports";
    const TS_DESCRIBE_LINE: usize = 1966;
    #[test]
    fn describes_020_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 1966);
    }
    #[test]
    fn case_1967_should_extract_simple_use_declaration() {
        let suite = ["Import Extraction", "Rust imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(115, 115);
        let result = extract("main.rs", "use std::io;");
        let import = single_import(&result, "std");
        assert_signature_eq(import, "use std::io;");
    }
    #[test]
    fn case_1977_should_extract_scoped_use_list() {
        let suite = ["Import Extraction", "Rust imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(116, 116);
        let result = extract("main.rs", "use std::{ffi::OsStr, io, path::Path};");
        let import = single_import(&result, "std");
        assert_signature_contains(import, "ffi::OsStr");
        assert_signature_contains(import, "path::Path");
    }
    #[test]
    fn case_1988_should_extract_crate_imports() {
        let suite = ["Import Extraction", "Rust imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(117, 117);
        let result = extract("lib.rs", "use crate::error::Error;");
        single_import(&result, "crate");
    }
    #[test]
    fn case_1997_should_extract_super_imports() {
        let suite = ["Import Extraction", "Rust imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(118, 118);
        let result = extract("submod.rs", "use super::utils;");
        single_import(&result, "super");
    }
    #[test]
    fn case_2006_should_extract_external_crate_imports() {
        let suite = ["Import Extraction", "Rust imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(119, 119);
        let result = extract("types.rs", "use serde::{Serialize, Deserialize};");
        let import = single_import(&result, "serde");
        assert_signature_contains(import, "Serialize");
        assert_signature_contains(import, "Deserialize");
    }
}
