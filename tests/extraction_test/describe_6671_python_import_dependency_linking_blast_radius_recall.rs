mod describe_6671_python_import_dependency_linking_blast_radius_recall {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Python import dependency linking (blast-radius recall)";
    const TS_DESCRIBE_LINE: usize = 6671;
    #[test]
    fn describes_106_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 6671);
    }
    #[test]
    fn case_6676_emits_an_imports_reference_per_name_in_a_from_module_import_incl_value() {
        let suite = ["Python import dependency linking (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(348, 348);
        let code = [
            "from foo import helper, widget",
            "from foo import Thing as T",
            "from . import sibling",
            "from bar import *",
        ]
        .join("\n");
        let result = extract("mod.py", &code);
        let names = reference_names(&result, ReferenceKind::Imports);
        for expected in ["helper", "widget", "T", "sibling"] {
            assert_contains(&names, expected);
        }
        assert_not_contains(&names, "*");
    }
    #[test]
    fn case_6693_a_python_value_imported_but_never_called_still_makes_the_importer_a_de() {
        let suite = ["Python import dependency linking (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(349, 349);
        let temp = TempDir::new("codegraph-python-import-dependent");
        temp.write(
            "pkg/foo.py",
            "widget = {\"n\": 1}\ndef helper():\n    return 1\n",
        );
        temp.write(
            "pkg/bar.py",
            "from foo import widget, helper\nregistry = [widget]\n",
        );
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("pkg/foo.py");
        assert_contains(&dependents, "pkg/bar.py");
    }
    #[test]
    fn case_6711_resolves_from_import_submodule_submodule_func_to_the_submodule() {
        let suite = ["Python import dependency linking (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(350, 350);
        let temp = TempDir::new("codegraph-python-submodule-dependent");
        temp.write("pkg/__init__.py", "");
        temp.write("pkg/certs.py", "def where():\n    return \"/ca.pem\"\n");
        temp.write(
            "pkg/utils.py",
            "from . import certs\ndef go():\n    return certs.where()\n",
        );
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("pkg/certs.py");
        assert_contains(&dependents, "pkg/utils.py");
    }
    #[test]
    fn case_6731_a_module_import_is_a_dependency_even_when_the_used_member_is_re_export() {
        let suite = ["Python import dependency linking (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(351, 351);
        let temp = TempDir::new("codegraph-python-reexport-dependent");
        temp.write("pkg/__init__.py", "");
        temp.write("pkg/certs.py", "from external_ca import where\n");
        temp.write("pkg/utils.py", "from . import certs\nCA = certs.where()\n");
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("pkg/certs.py");
        assert_contains(&dependents, "pkg/utils.py");
    }
}
