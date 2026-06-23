mod describe_3490_kotlin_multiplatform_expect_actual {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Kotlin Multiplatform expect/actual";
    const TS_DESCRIBE_LINE: usize = 3490;
    #[test]
    fn describes_052_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 3490);
    }
    #[test]
    fn case_3503_links_expect_declarations_to_platform_actual_implementations_and_surfa() {
        let suite = ["Kotlin Multiplatform expect/actual"];
        assert_eq!(suite.len(), 1);
        assert_eq!(216, 216);
        let temp = TempDir::new("codegraph-kotlin-expect-actual");
        temp.write(
            "src/commonMain/SystemProps.kt",
            r#"package demo.internal

expect fun systemProp(name: String): String?

expect class Platform {
    fun describe(): String
}
"#,
        );
        temp.write(
            "src/commonMain/Caller.kt",
            r#"package demo

import demo.internal.systemProp
import demo.internal.Platform

fun useIt(): String {
    val v = systemProp("os.name")
    return Platform().describe() + v
}
"#,
        );
        temp.write(
            "src/jvmMain/SystemProps.kt",
            r#"package demo.internal

actual fun systemProp(name: String): String? = System.getProperty(name)

actual class Platform {
    actual fun describe(): String = "JVM"
}
"#,
        );

        let mut cg = index_project(&temp);
        let fns = cg.get_nodes_by_kind(NodeKind::Function);
        let actual_fn = fns
            .iter()
            .find(|node| {
                node.name == "systemProp"
                    && node
                        .decorators
                        .as_ref()
                        .is_some_and(|decorators| decorators.iter().any(|d| d == "actual"))
            })
            .expect("actual systemProp should be indexed");
        let expect_fn = fns
            .iter()
            .find(|node| {
                node.name == "systemProp"
                    && node
                        .decorators
                        .as_ref()
                        .is_some_and(|decorators| decorators.iter().any(|d| d == "expect"))
            })
            .expect("expect systemProp should be indexed");
        assert_ne!(actual_fn.file_path, expect_fn.file_path);

        let actual_id = actual_fn.id.clone();
        let expect_id = expect_fn.id.clone();
        let impact = cg.get_impact_radius(&actual_id, 3);
        let impacted = impact
            .nodes
            .values()
            .map(|node| node.name.clone())
            .collect::<Vec<_>>();
        assert_contains(&impacted, "systemProp");
        assert_contains(&impacted, "useIt");
        let bridge = impact.edges.iter().any(|edge| {
            edge.source == expect_id
                && edge.target == actual_id
                && edge.provenance == Some(rustcodegraph::types::EdgeProvenance::Heuristic)
                && edge
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("synthesizedBy"))
                    .and_then(|value| value.as_str())
                    == Some("kotlin-expect-actual")
        });
        assert!(bridge, "missing expect/actual bridge: {:?}", impact.edges);
        cg.close();
    }
    #[test]
    fn case_3582_links_an_expect_class_to_an_actual_typealias_different_node_kinds() {
        let suite = ["Kotlin Multiplatform expect/actual"];
        assert_eq!(suite.len(), 1);
        assert_eq!(217, 217);
        let temp = TempDir::new("codegraph-kotlin-expect-actual-typealias");
        temp.write(
            "src/commonMain/Lock.kt",
            r#"package demo

expect class Lock {
    fun acquire()
}
"#,
        );
        temp.write(
            "src/jvmMain/Lock.kt",
            r#"package demo

actual typealias Lock = java.util.concurrent.locks.ReentrantLock
"#,
        );

        let mut cg = index_project(&temp);
        let alias = cg
            .get_nodes_by_kind(NodeKind::TypeAlias)
            .into_iter()
            .find(|node| {
                node.name == "Lock"
                    && node
                        .decorators
                        .as_ref()
                        .is_some_and(|decorators| decorators.iter().any(|d| d == "actual"))
            })
            .expect("actual Lock typealias should be indexed");
        let impact = cg.get_impact_radius(&alias.id, 3);
        let bridge = impact.edges.iter().any(|edge| {
            edge.target == alias.id
                && edge
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("synthesizedBy"))
                    .and_then(|value| value.as_str())
                    == Some("kotlin-expect-actual")
        });
        assert!(
            bridge,
            "missing expect/typealias bridge: {:?}",
            impact.edges
        );
        cg.close();
    }
}
