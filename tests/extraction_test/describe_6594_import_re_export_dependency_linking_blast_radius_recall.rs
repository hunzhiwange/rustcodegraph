mod describe_6594_import_re_export_dependency_linking_blast_radius_recall {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Import / re-export dependency linking (blast-radius recall)";
    const TS_DESCRIBE_LINE: usize = 6594;
    #[test]
    fn describes_105_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 6594);
    }
    #[test]
    fn case_6600_emits_an_imports_reference_per_named_aliased_and_default_import_bindin() {
        let suite = ["Import / re-export dependency linking (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(344, 344);
        let code = r#"
import { widget, helper as h } from './foo';
import Thing from './thing';
import * as NS from './ns';
export const registry = [widget];
"#;
        let result = extract("bar.ts", code);
        let names = reference_names(&result, ReferenceKind::Imports);
        for expected in ["widget", "h", "Thing", "NS"] {
            assert_contains(&names, expected);
        }
    }
    #[test]
    fn case_6617_emits_an_imports_reference_per_re_exported_binding() {
        let suite = ["Import / re-export dependency linking (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(345, 345);
        let result = extract("barrel.ts", "export { alpha, beta as b } from './source';");
        let names = reference_names(&result, ReferenceKind::Imports);
        assert_contains(&names, "alpha");
        assert_contains(&names, "beta");
        assert_not_contains(&names, "b");
    }
    #[test]
    fn case_6627_a_value_imported_re_exported_but_never_called_still_makes_the_importer() {
        let suite = ["Import / re-export dependency linking (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(346, 346);
        let temp = TempDir::new("codegraph-ts-import-dependent");
        temp.write(
            "src/foo.ts",
            "export const widget = { n: 1 };\nexport function helper(): void {}\n",
        );
        temp.write(
            "src/bar.ts",
            "import { widget } from './foo';\nexport { helper } from './foo';\nexport const registry = [widget];\n",
        );
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("src/foo.ts");
        assert_contains(&dependents, "src/bar.ts");
    }
    #[test]
    fn case_6651_a_namespace_import_touched_only_via_a_value_member_read_still_links_th() {
        let suite = ["Import / re-export dependency linking (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(347, 347);
        let temp = TempDir::new("codegraph-ts-namespace-dependent");
        temp.write("src/foo.ts", "export const SOME_CONST = 42;\n");
        temp.write(
            "src/bar.ts",
            "import * as foo from './foo';\nexport const x = foo.SOME_CONST;\n",
        );
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("src/foo.ts");
        assert_contains(&dependents, "src/bar.ts");
    }
}
