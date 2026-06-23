use super::*;

#[test]
fn resolves_callers_through_typed_object_pointers() {
    let project = TempProject::new("cg-cpp");
    project.write(
        "detect.hpp",
        "class CDetect {\n\
          public:\n\
           int Processing();\n\
         };\n\
         class CDetector {\n\
          private:\n\
           CDetect* m_cpAlg = nullptr;\n\
          public:\n\
           int Run();\n\
           int Flush();\n\
         };\n",
    );
    project.write(
        "detect.cpp",
        "#include \"detect.hpp\"\n\
         int CDetector::Run() { return m_cpAlg->Processing(); }\n\
         int CDetector::Flush() { return m_cpAlg->Processing(); }\n\
         int CDetect::Processing() { return 0; }\n",
    );

    let mut cg = index(&project);

    let processing = cg
        .get_nodes_by_kind(NodeKind::Method)
        .into_iter()
        .find(|node| node.qualified_name.ends_with("CDetect::Processing"))
        .expect("CDetect::Processing should be defined");

    let callers = cg
        .get_callers(&processing.id, 1)
        .into_iter()
        .map(|caller| caller.node.qualified_name)
        .collect::<Vec<_>>();
    assert!(callers.contains(&"CDetector::Run".to_owned()));
    assert!(callers.contains(&"CDetector::Flush".to_owned()));

    let run_method = cg
        .get_nodes_by_kind(NodeKind::Method)
        .into_iter()
        .find(|node| node.qualified_name.ends_with("CDetector::Run"))
        .expect("CDetector::Run should be defined");
    let callees = cg
        .get_callees(&run_method.id, 1)
        .into_iter()
        .map(|callee| callee.node.qualified_name)
        .collect::<Vec<_>>();
    assert!(callees.contains(&"CDetect::Processing".to_owned()));

    cg.close();
}

#[test]
fn resolves_typed_pointer_callers_when_the_method_name_is_ambiguous_and_the_call_sits_inside_a_return_declaration()
 {
    let project = TempProject::new("cg-cpp");
    project.write(
        "detect.hpp",
        "class CDetect { public: int Processing(); };\n\
         class CWidget { public: int Processing(); };\n\
         class CDetector {\n\
          private:\n\
           CDetect* m_cpAlg = nullptr;\n\
          public:\n\
           int RunReturn();\n\
           int RunAssign();\n\
         };\n",
    );
    project.write(
        "detect.cpp",
        "#include \"detect.hpp\"\n\
         int CDetector::RunReturn() { return m_cpAlg->Processing(); }\n\
         int CDetector::RunAssign() { int r = m_cpAlg->Processing(); return r; }\n\
         int CDetect::Processing() { return 0; }\n\
         int CWidget::Processing() { return 0; }\n",
    );

    let mut cg = index(&project);

    let methods = cg.get_nodes_by_kind(NodeKind::Method);
    let detect_proc = methods
        .iter()
        .find(|node| node.qualified_name == "CDetect::Processing")
        .expect("CDetect::Processing should be defined");
    let widget_proc = methods
        .iter()
        .find(|node| node.qualified_name == "CWidget::Processing")
        .expect("CWidget::Processing should be defined");

    let detect_callers = cg
        .get_callers(&detect_proc.id, 1)
        .into_iter()
        .map(|caller| caller.node.qualified_name)
        .collect::<Vec<_>>();
    assert!(detect_callers.contains(&"CDetector::RunReturn".to_owned()));
    assert!(detect_callers.contains(&"CDetector::RunAssign".to_owned()));

    let widget_callers = cg
        .get_callers(&widget_proc.id, 1)
        .into_iter()
        .map(|caller| caller.node.qualified_name)
        .collect::<Vec<_>>();
    assert!(!widget_callers.contains(&"CDetector::RunReturn".to_owned()));
    assert!(!widget_callers.contains(&"CDetector::RunAssign".to_owned()));

    cg.close();
}

#[test]
fn bridges_a_base_virtual_method_to_the_subclass_override() {
    let project = TempProject::new("cg-cpp");
    project.write(
        "iter.cpp",
        "class Iterator {\n\
          public:\n\
           virtual void Next() { }\n\
         };\n\
         class DBIter : public Iterator {\n\
          public:\n\
           void Next() override { advance(); }\n\
           void advance() { }\n\
         };\n",
    );

    let mut cg = index(&project);

    let mut nexts = cg
        .get_nodes_by_kind(NodeKind::Method)
        .into_iter()
        .filter(|node| node.name == "Next")
        .collect::<Vec<_>>();
    nexts.sort_by_key(|node| node.start_line);
    assert_eq!(nexts.len(), 2);
    let base_next = &nexts[0];
    let override_next = &nexts[1];

    let edge = cg
        .get_outgoing_edges(&base_next.id)
        .into_iter()
        .find(|edge| edge.target == override_next.id && edge.kind == EdgeKind::Calls);
    assert!(
        edge.is_some(),
        "Iterator::Next should reach DBIter::Next via override synthesis"
    );

    cg.close();
}
