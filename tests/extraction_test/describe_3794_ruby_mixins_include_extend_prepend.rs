mod describe_3794_ruby_mixins_include_extend_prepend {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Ruby mixins (include/extend/prepend)";
    const TS_DESCRIBE_LINE: usize = 3794;
    #[test]
    fn describes_055_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 3794);
    }
    #[test]
    fn case_3807_links_include_extend_prepend_to_the_mixed_in_module_across_files() {
        let suite = ["Ruby mixins (include/extend/prepend)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(220, 220);
        let temp = TempDir::new("codegraph-ruby-mixins");
        temp.write(
            "lib/concerns.rb",
            r#"module Trackable
  def track; end
end

module Cacheable
  def cache; end
end

module Loggable
  def log; end
end
"#,
        );
        temp.write(
            "lib/model.rb",
            r#"class Model
  include Trackable
  prepend Cacheable
  extend Loggable
end
"#,
        );

        let mut cg = index_project(&temp);
        let model = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.name == "Model")
            .expect("Model class should be indexed");
        assert!(model.file_path.ends_with("lib/model.rb"));
        for module_name in ["Trackable", "Cacheable", "Loggable"] {
            let module = cg
                .get_nodes_by_kind(NodeKind::Module)
                .into_iter()
                .find(|node| node.name == module_name)
                .unwrap_or_else(|| panic!("{module_name} module should be indexed"));
            let impacted = impact_names(&mut cg, &module.id, 3);
            assert_contains(&impacted, "Model");
        }
        cg.close();
    }
    #[test]
    fn case_3853_resolves_require_require_relative_to_the_required_file() {
        let suite = ["Ruby mixins (include/extend/prepend)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(221, 221);
        let temp = TempDir::new("codegraph-ruby-require");
        temp.write(
            "lib/app/fetcher.rb",
            r#"module App
  class Fetcher
    def fetch; end
  end
end
"#,
        );
        temp.write(
            "lib/app/worker.rb",
            r#"require "app/fetcher"

module App
  class Worker; end
end
"#,
        );
        temp.write(
            "lib/app/boot.rb",
            r#"require_relative "fetcher"
"#,
        );

        let mut cg = index_project(&temp);
        let fetcher = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("app/fetcher.rb"))
            .expect("fetcher.rb file node should be indexed");
        let reached = impact_file_paths(&mut cg, &fetcher.id, 2);
        assert!(
            reached.iter().any(|path| path.ends_with("app/worker.rb")),
            "worker.rb should depend on fetcher.rb: {reached:?}"
        );
        assert!(
            reached.iter().any(|path| path.ends_with("app/boot.rb")),
            "boot.rb should depend on fetcher.rb: {reached:?}"
        );
        cg.close();
    }
}
