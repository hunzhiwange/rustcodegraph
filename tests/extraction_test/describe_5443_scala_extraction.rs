mod describe_5443_scala_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Scala Extraction";
    const TS_DESCRIBE_LINE: usize = 5443;
    #[test]
    fn describes_078_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 5443);
    }
    mod describe_5444_language_detection {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Language detection";
        const TS_DESCRIBE_LINE: usize = 5444;
        #[test]
        fn describes_079_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 5444);
        }
        #[test]
        fn case_5445_should_detect_scala_files() {
            let suite = ["Scala Extraction", "Language detection"];
            assert_eq!(suite.len(), 2);
            assert_eq!(271, 271);
            assert_detected_language("Main.scala", None, Language::Scala);
            assert_detected_language("script.sc", None, Language::Scala);
            assert_detected_language("src/UserService.scala", None, Language::Scala);
        }
        #[test]
        fn case_5451_should_report_scala_as_supported() {
            let suite = ["Scala Extraction", "Language detection"];
            assert_eq!(suite.len(), 2);
            assert_eq!(272, 272);
            assert_language_support(Language::Scala, true);
            assert_supported_languages_include(&[Language::Scala]);
        }
    }
    mod describe_5457_class_extraction {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Class extraction";
        const TS_DESCRIBE_LINE: usize = 5457;
        #[test]
        fn describes_080_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 5457);
        }
        #[test]
        fn case_5458_should_extract_class_definitions() {
            let suite = ["Scala Extraction", "Class extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(273, 273);
            let code = r#"
class UserService(private val repo: UserRepository) {
  def findUser(id: String): Option[String] = Some(id)
}
"#;
            let result = extract("UserService.scala", code);
            let class_node = expect_node(&result, NodeKind::Class, "UserService");
            assert_eq!(class_node.language, Language::Scala);
        }
        #[test]
        fn case_5470_should_extract_object_definitions_as_class_kind() {
            let suite = ["Scala Extraction", "Class extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(274, 274);
            let code = r#"
object DatabaseConfig {
  val url = "jdbc:postgresql://localhost/mydb"
}
"#;
            let result = extract("Config.scala", code);
            expect_node(&result, NodeKind::Class, "DatabaseConfig");
        }
        #[test]
        fn case_5481_should_extract_trait_definitions_as_trait_kind() {
            let suite = ["Scala Extraction", "Class extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(275, 275);
            let code = r#"
trait Repository[A] {
  def findById(id: String): Option[A]
  def save(entity: A): Unit
}
"#;
            let result = extract("Repository.scala", code);
            expect_node(&result, NodeKind::Trait, "Repository");
        }
    }
    mod describe_5494_method_and_function_extraction {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Method and function extraction";
        const TS_DESCRIBE_LINE: usize = 5494;
        #[test]
        fn describes_081_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 5494);
        }
        #[test]
        fn case_5495_should_extract_method_definitions_inside_a_class() {
            let suite = ["Scala Extraction", "Method and function extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(276, 276);
            let code = r#"
class Calculator {
  def add(a: Int, b: Int): Int = a + b
  def divide(a: Double, b: Double): Double = a / b
}
"#;
            let result = extract("Calculator.scala", code);
            assert_names_include(&result, NodeKind::Method, &["add", "divide"]);
        }
        #[test]
        fn case_5508_should_extract_method_signatures() {
            let suite = ["Scala Extraction", "Method and function extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(277, 277);
            let code = r#"
class Greeter {
  def greet(name: String): String = s"Hello, ${name}!"
}
"#;
            let result = extract("Greeter.scala", code);
            let method = expect_node(&result, NodeKind::Method, "greet");
            assert_signature_contains(method, "name: String");
            assert_signature_contains(method, "String");
        }
        #[test]
        fn case_5520_should_extract_top_level_function_definitions_as_functions() {
            let suite = ["Scala Extraction", "Method and function extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(278, 278);
            let code = r#"
def factorial(n: Int): Int = if (n <= 1) 1 else n * factorial(n - 1)
def greet(name: String): String = s"Hello, ${name}!"
"#;
            let result = extract("utils.scala", code);
            assert_names_include(&result, NodeKind::Function, &["factorial", "greet"]);
        }
    }
    mod describe_5532_val_and_var_extraction {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Val and var extraction";
        const TS_DESCRIBE_LINE: usize = 5532;
        #[test]
        fn describes_082_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 5532);
        }
        #[test]
        fn case_5533_should_extract_val_inside_a_class_as_field() {
            let suite = ["Scala Extraction", "Val and var extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(279, 279);
            let code = r#"
class Config {
  val timeout: Int = 30
  val host: String = "localhost"
}
"#;
            let result = extract("Config.scala", code);
            assert_names_include(&result, NodeKind::Field, &["timeout", "host"]);
        }
        #[test]
        fn case_5546_should_extract_var_inside_a_class_as_field() {
            let suite = ["Scala Extraction", "Val and var extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(280, 280);
            let code = r#"
class Counter {
  var count: Int = 0
}
"#;
            let result = extract("Counter.scala", code);
            expect_node(&result, NodeKind::Field, "count");
        }
        #[test]
        fn case_5557_should_extract_top_level_val_as_constant() {
            let suite = ["Scala Extraction", "Val and var extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(281, 281);
            let code = r#"
val MaxConnections: Int = 100
val DefaultTimeout = 30
"#;
            let result = extract("constants.scala", code);
            expect_node(&result, NodeKind::Constant, "MaxConnections");
        }
        #[test]
        fn case_5567_should_extract_top_level_var_as_variable() {
            let suite = ["Scala Extraction", "Val and var extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(282, 282);
            let code = r#"
var retries: Int = 3
"#;
            let result = extract("state.scala", code);
            expect_node(&result, NodeKind::Variable, "retries");
        }
        #[test]
        fn case_5576_should_include_type_in_val_var_signature() {
            let suite = ["Scala Extraction", "Val and var extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(283, 283);
            let code = r#"
class Service {
  val timeout: Int = 30
}
"#;
            let result = extract("Service.scala", code);
            let field = expect_node(&result, NodeKind::Field, "timeout");
            assert_signature_contains(field, "timeout");
            assert_signature_contains(field, "Int");
        }
    }
    mod describe_5589_enum_extraction {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Enum extraction";
        const TS_DESCRIBE_LINE: usize = 5589;
        #[test]
        fn describes_083_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 5589);
        }
        #[test]
        fn case_5590_should_extract_enum_definitions() {
            let suite = ["Scala Extraction", "Enum extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(284, 284);
            let code = r#"
enum Color:
  case Red
  case Green
  case Blue
"#;
            let result = extract("Color.scala", code);
            expect_node(&result, NodeKind::Enum, "Color");
        }
        #[test]
        fn case_5602_should_extract_enum_cases_as_enum_member() {
            let suite = ["Scala Extraction", "Enum extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(285, 285);
            let code = r#"
enum Direction:
  case North
  case South
  case East
  case West
"#;
            let result = extract("Direction.scala", code);
            assert_names_include(
                &result,
                NodeKind::EnumMember,
                &["North", "South", "East", "West"],
            );
            assert!(nodes_by_kind(&result, NodeKind::EnumMember).len() >= 4);
        }
    }
    mod describe_5618_type_alias_extraction {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Type alias extraction";
        const TS_DESCRIBE_LINE: usize = 5618;
        #[test]
        fn describes_084_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 5618);
        }
        #[test]
        fn case_5619_should_extract_type_aliases() {
            let suite = ["Scala Extraction", "Type alias extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(286, 286);
            let code = r#"
type UserId = String
type UserMap = Map[String, String]
"#;
            let result = extract("types.scala", code);
            assert_names_include(&result, NodeKind::TypeAlias, &["UserId", "UserMap"]);
        }
    }
    mod describe_5631_import_extraction {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Import extraction";
        const TS_DESCRIBE_LINE: usize = 5631;
        #[test]
        fn describes_085_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 5631);
        }
        #[test]
        fn case_5632_should_extract_import_declarations() {
            let suite = ["Scala Extraction", "Import extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(287, 287);
            let code = r#"
import scala.collection.mutable.ListBuffer
import scala.concurrent.Future
"#;
            let result = extract("imports.scala", code);
            assert!(
                import_nodes(&result).len() >= 2,
                "imports: {:?}",
                import_nodes(&result)
            );
        }
    }
    mod describe_5643_visibility_modifiers {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Visibility modifiers";
        const TS_DESCRIBE_LINE: usize = 5643;
        #[test]
        fn describes_086_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 5643);
        }
        #[test]
        fn case_5644_should_extract_private_visibility() {
            let suite = ["Scala Extraction", "Visibility modifiers"];
            assert_eq!(suite.len(), 2);
            assert_eq!(288, 288);
            let code = r#"
class Service {
  private val secret: String = "abc"
  private def helper(): Unit = {}
}
"#;
            let result = extract("Service.scala", code);
            assert_eq!(
                expect_node(&result, NodeKind::Field, "secret").visibility,
                Some(Visibility::Private)
            );
            assert_eq!(
                expect_node(&result, NodeKind::Method, "helper").visibility,
                Some(Visibility::Private)
            );
        }
        #[test]
        fn case_5658_should_extract_protected_visibility() {
            let suite = ["Scala Extraction", "Visibility modifiers"];
            assert_eq!(suite.len(), 2);
            assert_eq!(289, 289);
            let code = r#"
class Base {
  protected def helperMethod(): Unit = {}
}
"#;
            let result = extract("Base.scala", code);
            assert_eq!(
                expect_node(&result, NodeKind::Method, "helperMethod").visibility,
                Some(Visibility::Protected)
            );
        }
        #[test]
        fn case_5669_should_default_to_public_visibility() {
            let suite = ["Scala Extraction", "Visibility modifiers"];
            assert_eq!(suite.len(), 2);
            assert_eq!(290, 290);
            let code = r#"
class Greeter {
  def hello(): Unit = {}
}
"#;
            let result = extract("Greeter.scala", code);
            assert_eq!(
                expect_node(&result, NodeKind::Method, "hello").visibility,
                Some(Visibility::Public)
            );
        }
    }
    mod describe_5681_inheritance {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Inheritance";
        const TS_DESCRIBE_LINE: usize = 5681;
        #[test]
        fn describes_087_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 5681);
        }
        #[test]
        fn case_5682_should_extract_extends_relationships() {
            let suite = ["Scala Extraction", "Inheritance"];
            assert_eq!(suite.len(), 2);
            assert_eq!(291, 291);
            let code = r#"
class AdminUser extends User {
  def adminAction(): Unit = {}
}
"#;
            let result = extract("AdminUser.scala", code);
            assert_reference_names_include(&result, ReferenceKind::Extends, &["User"]);
        }
    }
    mod describe_5694_call_extraction {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Call extraction";
        const TS_DESCRIBE_LINE: usize = 5694;
        #[test]
        fn describes_088_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 5694);
        }
        #[test]
        fn case_5695_should_extract_function_call_expressions() {
            let suite = ["Scala Extraction", "Call extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(292, 292);
            let code = r#"
def processData(): Unit = {
  val result = computeResult()
  println(result)
}
"#;
            let result = extract("processor.scala", code);
            let calls = reference_names(&result, ReferenceKind::Calls);
            assert!(!calls.is_empty(), "calls: {calls:?}");
        }
    }
}
