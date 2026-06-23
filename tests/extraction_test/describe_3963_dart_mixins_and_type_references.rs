mod describe_3963_dart_mixins_and_type_references {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Dart mixins and type references";
    const TS_DESCRIBE_LINE: usize = 3963;
    #[test]
    fn describes_057_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 3963);
    }
    #[test]
    fn case_3976_links_with_mixins_and_method_parameter_return_types_across_files() {
        let suite = ["Dart mixins and type references"];
        assert_eq!(suite.len(), 1);
        assert_eq!(223, 223);
        let temp = TempDir::new("codegraph-dart-mixins-types");
        temp.write(
            "lib/models.dart",
            r#"class User {
  final String name;
  User(this.name);
}

mixin Loggable {
  void log() {}
}

abstract class Repository {
  User find(int id);
}
"#,
        );
        temp.write(
            "lib/service.dart",
            r#"import 'models.dart';

class UserService extends Repository with Loggable {
  @override
  User find(int id) => User('x');

  List<User> all() => [];
}
"#,
        );

        let mut cg = index_project(&temp);
        let loggable = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .chain(cg.get_nodes_by_kind(NodeKind::Module))
            .find(|node| node.name == "Loggable")
            .expect("Loggable mixin should be indexed");
        let mixin_users = impact_names(&mut cg, &loggable.id, 3);
        assert_contains(&mixin_users, "UserService");

        let user = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.name == "User" && node.file_path.ends_with("models.dart"))
            .expect("User class should be indexed");
        let user_deps = impact_file_paths(&mut cg, &user.id, 3);
        assert!(
            user_deps.iter().any(|path| path.ends_with("service.dart")),
            "User type references should reach service.dart: {user_deps:?}"
        );
        cg.close();
    }
}
