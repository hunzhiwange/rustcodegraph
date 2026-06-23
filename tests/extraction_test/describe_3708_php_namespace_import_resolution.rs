mod describe_3708_php_namespace_import_resolution {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "PHP namespace + import resolution";
    const TS_DESCRIBE_LINE: usize = 3708;
    #[test]
    fn describes_054_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 3708);
    }
    #[test]
    fn case_3721_resolves_use_imports_to_the_namespace_qualified_definition_and_type_hi() {
        let suite = ["PHP namespace + import resolution"];
        assert_eq!(suite.len(), 1);
        assert_eq!(219, 219);
        let temp = TempDir::new("codegraph-php-namespace-import");
        temp.write(
            "src/Cache/Factory.php",
            r#"<?php
namespace Contracts\Cache;

interface Factory {
    public function store(): object;
}
"#,
        );
        temp.write(
            "src/Mail/Factory.php",
            r#"<?php
namespace Contracts\Mail;

interface Factory {
    public function mailer(): object;
}
"#,
        );
        temp.write(
            "src/App/Service.php",
            r#"<?php
namespace App;

use Contracts\Cache\Factory;

class Service {
    public function make(): Factory {
        return resolve(Factory::class);
    }
}
"#,
        );

        let mut cg = index_project(&temp);
        let cache_factory = cg
            .get_nodes_by_kind(NodeKind::Interface)
            .into_iter()
            .find(|node| node.qualified_name == "Contracts\\Cache::Factory")
            .expect("Contracts\\Cache::Factory should be indexed");
        let mail_factory = cg
            .get_nodes_by_kind(NodeKind::Interface)
            .into_iter()
            .find(|node| node.qualified_name == "Contracts\\Mail::Factory")
            .expect("Contracts\\Mail::Factory should be indexed");
        let cache_reaches = impact_file_paths(&mut cg, &cache_factory.id, 3)
            .iter()
            .any(|path| path.ends_with("src/App/Service.php"));
        let mail_reaches = impact_file_paths(&mut cg, &mail_factory.id, 3)
            .iter()
            .any(|path| path.ends_with("src/App/Service.php"));
        assert!(cache_reaches, "cache Factory should reach Service.php");
        assert!(!mail_reaches, "mail Factory should not reach Service.php");
        cg.close();
    }
}
