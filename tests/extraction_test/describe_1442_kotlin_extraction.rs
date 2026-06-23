mod describe_1442_kotlin_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Kotlin Extraction";
    const TS_DESCRIBE_LINE: usize = 1442;
    #[test]
    fn describes_015_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 1442);
    }
    #[test]
    fn case_1443_should_extract_class_declarations() {
        let suite = ["Kotlin Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(80, 80);
        let code = r#"
class UserRepository(private val database: Database) {
    fun findById(id: String): User? {
        return database.query("SELECT * FROM users WHERE id = ?", id)
    }

    suspend fun save(user: User) {
        database.insert(user)
    }
}
"#;
        let result = extract("UserRepository.kt", code);
        let class_node = find_node(&result, NodeKind::Class, "UserRepository")
            .expect("UserRepository class should be extracted");
        assert_eq!(class_node.language, Language::Kotlin);
    }
    #[test]
    fn case_1462_should_extract_function_declarations() {
        let suite = ["Kotlin Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(81, 81);
        let code = r#"
fun calculateTotal(items: List<Item>): Double {
    return items.sumOf { it.price }
}

suspend fun fetchUserData(userId: String): User {
    return api.getUser(userId)
}
"#;
        let result = extract("utils.kt", code);
        let functions = names_by_kind(&result, NodeKind::Function);
        assert_contains(&functions, "calculateTotal");
        assert_contains(&functions, "fetchUserData");
    }
    #[test]
    fn case_1478_should_detect_suspend_functions_as_async() {
        let suite = ["Kotlin Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(82, 82);
        let code = r#"
suspend fun loadData(): List<String> {
    delay(1000)
    return listOf("a", "b", "c")
}
"#;
        let result = extract("loader.kt", code);
        let func_node = find_node(&result, NodeKind::Function, "loadData")
            .expect("loadData function should be extracted");
        assert_eq!(func_node.is_async, Some(true));
    }
    #[test]
    fn case_1492_should_extract_fun_interface_declarations() {
        let suite = ["Kotlin Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(83, 83);
        let code = r#"
fun interface OnObjectRetainedListener {
  fun onObjectRetained()
}
"#;
        let result = extract("listener.kt", code);
        find_node(&result, NodeKind::Interface, "OnObjectRetainedListener")
            .expect("fun interface should be extracted");
        let method = find_node(&result, NodeKind::Method, "onObjectRetained")
            .expect("SAM method should be extracted");
        assert!(method
            .qualified_name
            .ends_with("OnObjectRetainedListener::onObjectRetained"));
    }
    #[test]
    fn case_1510_should_extract_complex_fun_interface_with_nested_classes() {
        let suite = ["Kotlin Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(84, 84);
        let code = r#"
fun interface EventListener {
  fun onEvent(event: Event)

  sealed class Event {
    class DumpingHeap : Event()
  }
}
"#;
        let result = extract("events.kt", code);
        find_node(&result, NodeKind::Interface, "EventListener")
            .expect("EventListener interface should be extracted");
        find_node(&result, NodeKind::Class, "Event")
            .expect("nested Event class should be extracted");
        find_node(&result, NodeKind::Class, "DumpingHeap")
            .expect("nested DumpingHeap class should be extracted");
    }
    #[test]
    fn case_1534_should_not_affect_regular_function_declarations() {
        let suite = ["Kotlin Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(85, 85);
        let code = r#"
fun interface MyCallback {
  fun invoke(value: Int)
}

fun regularFunction(): String {
  return "hello"
}
"#;
        let result = extract("mixed.kt", code);
        find_node(&result, NodeKind::Interface, "MyCallback")
            .expect("fun interface should be extracted");
        find_node(&result, NodeKind::Function, "regularFunction")
            .expect("regular function should stay a function");
    }
    #[test]
    fn case_1555_should_extract_fun_interface_with_annotation_on_method_pattern_2b() {
        let suite = ["Kotlin Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(86, 86);
        let code = r#"
import java.io.IOException

fun interface Interceptor {
  @Throws(IOException::class)
  fun intercept(chain: Chain): Response
}
"#;
        let result = extract("interceptor.kt", code);
        find_node(&result, NodeKind::Interface, "Interceptor")
            .expect("annotated fun interface should be extracted");
    }
    #[test]
    fn case_1574_should_extract_methods_from_interface_with_nested_fun_interface() {
        let suite = ["Kotlin Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(87, 87);
        let code = r#"
interface WebSocket {
  fun request(): Request
  fun send(text: String): Boolean
  fun cancel()
  fun interface Factory {
    fun newWebSocket(request: Request): WebSocket
  }
}
"#;
        let result = extract("websocket.kt", code);
        find_node(&result, NodeKind::Interface, "WebSocket")
            .expect("WebSocket interface should be extracted");
        let methods = result
            .nodes
            .iter()
            .filter(|node| {
                node.kind == NodeKind::Method && node.qualified_name.contains("WebSocket::")
            })
            .map(|node| node.name.clone())
            .collect::<Vec<_>>();
        for name in ["request", "send", "cancel"] {
            assert_contains(&methods, name);
        }
    }
    #[test]
    fn case_1599_wraps_top_level_declarations_in_a_namespace_from_package_header() {
        let suite = ["Kotlin Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(88, 88);
        let code = r#"
package com.example.foo

class Bar {
  fun greet(): String = "hi"
}

fun util(): Int = 42
"#;
        let result = extract("Bar.kt", code);
        find_node(&result, NodeKind::Namespace, "com.example.foo")
            .expect("package namespace should be extracted");
        let class_node =
            find_node(&result, NodeKind::Class, "Bar").expect("Bar class should be extracted");
        assert!(class_node.qualified_name.ends_with("com.example.foo::Bar"));
        let greet = find_node(&result, NodeKind::Method, "greet")
            .expect("greet method should be extracted");
        assert!(greet
            .qualified_name
            .ends_with("com.example.foo::Bar::greet"));
        let util = find_node(&result, NodeKind::Function, "util")
            .expect("util function should be extracted");
        assert!(util.qualified_name.ends_with("com.example.foo::util"));
    }
    #[test]
    fn case_1624_handles_a_single_segment_package() {
        let suite = ["Kotlin Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(89, 89);
        let code = r#"
package foo

class Bar
"#;
        let result = extract("Bar.kt", code);
        let class_node =
            find_node(&result, NodeKind::Class, "Bar").expect("Bar class should be extracted");
        assert!(class_node.qualified_name.ends_with("foo::Bar"));
    }
    #[test]
    fn case_1635_does_not_wrap_when_no_package_is_declared() {
        let suite = ["Kotlin Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(90, 90);
        let code = r#"
class Bar {
  fun greet() = "hi"
}
"#;
        let result = extract("Bar.kt", code);
        assert!(
            result
                .nodes
                .iter()
                .all(|node| node.kind != NodeKind::Namespace),
            "nodes: {:?}",
            result.nodes
        );
        let class_node =
            find_node(&result, NodeKind::Class, "Bar").expect("Bar class should be extracted");
        assert!(class_node.qualified_name.ends_with("Bar"));
    }
}
