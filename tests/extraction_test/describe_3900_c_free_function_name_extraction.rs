mod describe_3900_c_free_function_name_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "C++ free-function name extraction";
    const TS_DESCRIBE_LINE: usize = 3900;
    #[test]
    fn describes_056_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 3900);
    }
    #[test]
    fn case_3913_names_a_free_function_correctly_when_it_has_qualified_type_params_or_a() {
        let suite = ["C++ free-function name extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(222, 222);
        let temp = TempDir::new("codegraph-cpp-free-function-names");
        temp.write(
            "src/names.cc",
            r#"#include <string>

std::string TableFileName(const std::string& dbname, int number) {
  return dbname;
}

auto BuildName(const std::string& a) -> std::string {
  return a;
}
"#,
        );
        temp.write(
            "src/user.cc",
            r#"#include <string>

std::string use() {
  return TableFileName("db", 1) + BuildName("x");
}
"#,
        );

        let mut cg = index_project(&temp);
        let fns = cg.get_nodes_by_kind(NodeKind::Function);
        let table_fn = fns
            .iter()
            .find(|node| node.name == "TableFileName")
            .expect("TableFileName should be indexed");
        let build_fn = fns
            .iter()
            .find(|node| node.name == "BuildName")
            .expect("BuildName should be indexed");
        for node in [table_fn, build_fn] {
            let reached = impact_file_paths(&mut cg, &node.id, 3);
            assert!(
                reached.iter().any(|path| path.ends_with("src/user.cc")),
                "{} should be called from user.cc: {reached:?}",
                node.name
            );
        }
        cg.close();
    }
}
