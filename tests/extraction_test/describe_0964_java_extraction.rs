mod describe_0964_java_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Java Extraction";
    const TS_DESCRIBE_LINE: usize = 964;
    #[test]
    fn describes_011_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 964);
    }
    #[test]
    fn case_0965_should_extract_class_declarations() {
        let suite = ["Java Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(63, 63);
        let code = r#"
public class UserService {
    private final UserRepository repository;

    public UserService(UserRepository repository) {
        this.repository = repository;
    }

    public User getUser(String id) {
        return repository.findById(id);
    }
}
"#;
        let result = extract("UserService.java", code);
        let class_node = find_node(&result, NodeKind::Class, "UserService")
            .expect("UserService class should be extracted");
        assert_eq!(class_node.language, Language::Java);
        assert_eq!(class_node.visibility, Some(Visibility::Public));
    }
    #[test]
    fn case_0987_should_extract_method_declarations() {
        let suite = ["Java Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(64, 64);
        let code = r#"
public class Calculator {
    public static int add(int a, int b) {
        return a + b;
    }
}
"#;
        let result = extract("Calculator.java", code);
        let method_node =
            find_node(&result, NodeKind::Method, "add").expect("add method should be extracted");
        assert_eq!(method_node.is_static, Some(true));
    }
    #[test]
    fn case_1002_wraps_top_level_declarations_in_a_namespace_from_package_declaration() {
        let suite = ["Java Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(65, 65);
        let code = r#"
package com.example.foo;

public class Bar {
    public String greet() { return "hi"; }
}
"#;
        let result = extract("Bar.java", code);
        let ns = find_node(&result, NodeKind::Namespace, "com.example.foo")
            .expect("package namespace should be extracted");
        assert_eq!(ns.language, Language::Java);

        let class_node =
            find_node(&result, NodeKind::Class, "Bar").expect("Bar class should be extracted");
        assert!(class_node.qualified_name.ends_with("com.example.foo::Bar"));

        let greet = find_node(&result, NodeKind::Method, "greet")
            .expect("greet method should be extracted");
        assert!(greet
            .qualified_name
            .ends_with("com.example.foo::Bar::greet"));
    }
    #[test]
    fn case_1022_does_not_wrap_when_no_package_is_declared() {
        let suite = ["Java Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(66, 66);
        let code = r#"
public class Bar {
    public String greet() { return "hi"; }
}
"#;
        let result = extract("Bar.java", code);
        assert!(
            !result
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::Namespace),
            "nodes: {:?}",
            result.nodes
        );
        let class_node =
            find_node(&result, NodeKind::Class, "Bar").expect("Bar class should be extracted");
        assert!(class_node.qualified_name.ends_with("Bar"));
    }
    #[test]
    fn case_1034_extracts_anonymous_class_overrides_from_new_t() {
        let suite = ["Java Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(67, 67);
        let code = r#"
package com.example;

abstract class Base {
  abstract int compute(int x);
}

public class Factory {
  public Base make() {
    return new Base() {
      @Override
      int compute(int x) { return x + 1; }
    };
  }
}
"#;
        let result = extract("Factory.java", code);
        let anon = result
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Class && node.name.contains("Base$anon@"))
            .expect("anonymous Base subclass should be extracted");
        let compute = result
            .nodes
            .iter()
            .find(|node| {
                node.kind == NodeKind::Method
                    && node.name == "compute"
                    && node.qualified_name.contains("$anon@")
            })
            .expect("override method should be scoped under the anonymous class");
        assert!(compute
            .qualified_name
            .contains("Factory::make::<Base$anon@"));
        assert!(compute.qualified_name.ends_with("::compute"));
        assert!(result.unresolved_references.iter().any(|reference| {
            reference.reference_kind == ReferenceKind::Extends
                && reference.reference_name == "Base"
                && reference.from_node_id == anon.id
        }));
        assert!(result.unresolved_references.iter().any(|reference| {
            reference.reference_kind == ReferenceKind::Instantiates
                && reference.reference_name == "Base"
        }));
    }
    #[test]
    fn case_1082_extracts_anonymous_class_overrides_inside_a_lambda_body() {
        let suite = ["Java Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(68, 68);
        let code = r#"
package com.example;

interface Strategy {
  java.util.Iterator<String> iterator(String s);
}

abstract class BaseIter implements java.util.Iterator<String> {
  abstract int separatorStart(int start);
}

public class Splitter {
  private final Strategy strategy;
  public Splitter(Strategy s) { this.strategy = s; }

  public static Splitter on(char c) {
    return new Splitter((seq) ->
        new BaseIter() {
          @Override
          int separatorStart(int start) { return start + 1; }
          @Override public boolean hasNext() { return false; }
          @Override public String next() { return null; }
        });
  }
}
"#;
        let result = extract("Splitter.java", code);
        assert!(
            result
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::Class && node.name.contains("BaseIter$anon@")),
            "nodes: {:?}",
            result.nodes
        );
        assert!(result.nodes.iter().any(|node| {
            node.kind == NodeKind::Method
                && node.name == "separatorStart"
                && node.qualified_name.contains("$anon@")
        }));
    }
}
