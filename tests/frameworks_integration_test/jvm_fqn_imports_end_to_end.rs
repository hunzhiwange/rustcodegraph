use super::*;

#[test]
fn resolves_a_kotlin_import_when_the_file_name_differs_from_the_class_name() {
    let project = TempProject::new("cg-jvm-imp");
    project.write(
        "Models.kt",
        "package com.example\n\nclass Bar {\n  fun greet(): String = \"hi\"\n}\n",
    );
    project.write(
        "Caller.kt",
        "package com.example.app\n\nimport com.example.Bar\n\nclass App {\n  fun run() { Bar().greet() }\n}\n",
    );

    let mut cg = index(&project);

    let bar = cg
        .get_nodes_by_kind(NodeKind::Class)
        .into_iter()
        .find(|node| node.qualified_name == "com.example::Bar")
        .expect("Bar should be extracted with package-qualified name");
    let import_node = cg
        .get_nodes_by_kind(NodeKind::Import)
        .into_iter()
        .find(|node| node.name == "com.example.Bar");
    assert!(import_node.is_some(), "import statement node should exist");

    let reaches_bar = cg
        .get_incoming_edges(&bar.id)
        .into_iter()
        .find(|edge| edge.kind == EdgeKind::Imports);
    assert!(
        reaches_bar.is_some(),
        "an imports edge should resolve to Bar via FQN"
    );

    cg.close();
}

#[test]
fn resolves_a_kotlin_top_level_function_import() {
    let project = TempProject::new("cg-jvm-imp");
    project.write("Utils.kt", "package com.example\n\nfun util(): Int = 42\n");
    project.write(
        "Caller.kt",
        "package com.example.app\n\nimport com.example.util\n\nfun main() { util() }\n",
    );

    let mut cg = index(&project);

    let util = cg
        .get_nodes_by_kind(NodeKind::Function)
        .into_iter()
        .find(|node| node.qualified_name == "com.example::util")
        .expect("top-level util() should be extracted under com.example");

    let edge = cg
        .get_incoming_edges(&util.id)
        .into_iter()
        .find(|edge| edge.kind == EdgeKind::Imports);
    assert!(
        edge.is_some(),
        "imports edge should reach the top-level function by FQN"
    );

    cg.close();
}

#[test]
fn resolves_cross_language_kotlin_importing_a_java_class() {
    let project = TempProject::new("cg-jvm-imp");
    project.write(
        "JavaBar.java",
        "package com.example;\n\npublic class JavaBar {\n  public String greet() { return \"hi\"; }\n}\n",
    );
    project.write(
        "Caller.kt",
        "package com.example.app\n\nimport com.example.JavaBar\n\nfun main() { JavaBar().greet() }\n",
    );

    let mut cg = index(&project);

    let java_bar = cg
        .get_nodes_by_kind(NodeKind::Class)
        .into_iter()
        .find(|node| node.qualified_name == "com.example::JavaBar")
        .expect("JavaBar should be extracted under com.example regardless of language");

    let edge = cg
        .get_incoming_edges(&java_bar.id)
        .into_iter()
        .find(|edge| edge.kind == EdgeKind::Imports);
    assert!(
        edge.is_some(),
        "Kotlin caller should resolve its import to the Java class"
    );

    cg.close();
}

#[test]
fn disambiguates_a_class_name_collision_across_packages() {
    let project = TempProject::new("cg-jvm-imp");
    project.write(
        "AlphaBar.kt",
        "package com.example.alpha\n\nclass Bar { fun who() = \"alpha\" }\n",
    );
    project.write(
        "BetaBar.kt",
        "package com.example.beta\n\nclass Bar { fun who() = \"beta\" }\n",
    );
    project.write(
        "CallerA.kt",
        "package app\n\nimport com.example.alpha.Bar\n\nfun a() { Bar().who() }\n",
    );
    project.write(
        "CallerB.kt",
        "package app\n\nimport com.example.beta.Bar\n\nfun b() { Bar().who() }\n",
    );

    let mut cg = index(&project);

    let alpha_bar = cg
        .get_nodes_by_kind(NodeKind::Class)
        .into_iter()
        .find(|node| node.qualified_name == "com.example.alpha::Bar")
        .expect("alpha Bar should exist");
    let beta_bar = cg
        .get_nodes_by_kind(NodeKind::Class)
        .into_iter()
        .find(|node| node.qualified_name == "com.example.beta::Bar")
        .expect("beta Bar should exist");
    assert_ne!(alpha_bar.id, beta_bar.id);

    let alpha_incoming = cg
        .get_incoming_edges(&alpha_bar.id)
        .into_iter()
        .filter(|edge| edge.kind == EdgeKind::Imports)
        .collect::<Vec<_>>();
    let beta_incoming = cg
        .get_incoming_edges(&beta_bar.id)
        .into_iter()
        .filter(|edge| edge.kind == EdgeKind::Imports)
        .collect::<Vec<_>>();
    assert!(!alpha_incoming.is_empty());
    assert!(!beta_incoming.is_empty());

    let alpha_source_files = alpha_incoming
        .iter()
        .filter_map(|edge| cg.get_node(&edge.source).map(|node| node.file_path))
        .collect::<Vec<_>>();
    let beta_source_files = beta_incoming
        .iter()
        .filter_map(|edge| cg.get_node(&edge.source).map(|node| node.file_path))
        .collect::<Vec<_>>();
    assert!(
        alpha_source_files
            .iter()
            .any(|path| path.contains("CallerA.kt"))
    );
    assert!(
        beta_source_files
            .iter()
            .any(|path| path.contains("CallerB.kt"))
    );

    cg.close();
}
