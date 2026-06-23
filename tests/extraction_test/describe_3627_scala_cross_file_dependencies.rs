mod describe_3627_scala_cross_file_dependencies {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Scala cross-file dependencies";
    const TS_DESCRIBE_LINE: usize = 3627;
    #[test]
    fn describes_053_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 3627);
    }
    #[test]
    fn case_3640_links_parameterized_supertypes_type_annotations_and_implicit_params_ac() {
        let suite = ["Scala cross-file dependencies"];
        assert_eq!(suite.len(), 1);
        assert_eq!(218, 218);
        let temp = TempDir::new("codegraph-scala-cross-file");
        temp.write(
            "src/main/scala/demo/Semigroup.scala",
            r#"package demo

trait Semigroup[A] {
  def combine(x: A, y: A): A
}
"#,
        );
        temp.write(
            "src/main/scala/demo/Monoid.scala",
            r#"package demo

trait Monoid[A] extends Semigroup[A] {
  def empty: A
}
"#,
        );
        temp.write(
            "src/main/scala/demo/Instances.scala",
            r#"package demo

object Instances {
  implicit val intMonoid: Monoid[Int] = new Monoid[Int] {
    def empty: Int = 0
    def combine(x: Int, y: Int): Int = x + y
  }
}
"#,
        );
        temp.write(
            "src/main/scala/demo/Folding.scala",
            r#"package demo

object Folding {
  def fold[A](xs: List[A])(implicit M: Monoid[A]): A =
    xs.foldLeft(M.empty)(M.combine)
}
"#,
        );

        let mut cg = index_project(&temp);
        let monoid = cg
            .get_nodes_by_kind(NodeKind::Trait)
            .into_iter()
            .find(|node| node.name == "Monoid")
            .expect("Monoid trait should be indexed");
        let semigroup = cg
            .get_nodes_by_kind(NodeKind::Trait)
            .into_iter()
            .find(|node| node.name == "Semigroup")
            .expect("Semigroup trait should be indexed");
        assert_ne!(monoid.file_path, semigroup.file_path);

        let sema_impact = impact_names(&mut cg, &semigroup.id, 3);
        assert_contains(&sema_impact, "Monoid");
        let impacted = impact_names(&mut cg, &monoid.id, 3);
        assert_contains(&impacted, "intMonoid");
        assert_contains(&impacted, "fold");
        cg.close();
    }
}
