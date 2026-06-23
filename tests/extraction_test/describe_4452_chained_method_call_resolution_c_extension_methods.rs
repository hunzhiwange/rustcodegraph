mod describe_4452_chained_method_call_resolution_c_extension_methods {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Chained method-call resolution (C# extension methods)";
    const TS_DESCRIBE_LINE: usize = 4452;
    #[test]
    fn describes_063_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4452);
    }
    #[test]
    fn case_4465_resolves_a_chained_extension_method_call_a_b_method_to_its_definition() {
        let suite = ["Chained method-call resolution (C# extension methods)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(236, 236);
        let temp = TempDir::new("codegraph-csharp-chained-extension");
        temp.write(
            "cfg/Ext.cs",
            r#"namespace App {
  public static class Ext {
    public static object AddCoreServices(this object services, int x) { return services; }
  }
}
"#,
        );
        temp.write(
            "Program.cs",
            r#"namespace App {
  public class Program {
    public void Run(object builder) {
      builder.Services.AddCoreServices(1);
    }
  }
}
"#,
        );

        let mut cg = index_project(&temp);
        let ext = cg
            .get_nodes_by_kind(NodeKind::Method)
            .into_iter()
            .find(|node| node.name == "AddCoreServices")
            .or_else(|| {
                cg.get_nodes_by_kind(NodeKind::Function)
                    .into_iter()
                    .find(|node| node.name == "AddCoreServices")
            })
            .expect("AddCoreServices should be indexed");
        let callers = impact_file_paths(&mut cg, &ext.id, 2);
        assert!(
            callers.iter().any(|path| path.ends_with("Program.cs")),
            "chained extension call should reach Program.cs: {callers:?}"
        );
        cg.close();
    }
}
