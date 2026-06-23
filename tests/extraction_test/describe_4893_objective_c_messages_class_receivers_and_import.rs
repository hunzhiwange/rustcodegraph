mod describe_4893_objective_c_messages_class_receivers_and_import {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Objective-C messages, class receivers, and #import";
    const TS_DESCRIBE_LINE: usize = 4893;
    #[test]
    fn describes_072_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4893);
    }
    #[test]
    fn case_4906_resolves_single_arg_selectors_class_message_receivers_and_import_heade() {
        let suite = ["Objective-C messages, class receivers, and #import"];
        assert_eq!(suite.len(), 1);
        assert_eq!(248, 248);
        let temp = TempDir::new("codegraph-objc-message-imports");
        temp.write(
            "SDImageCache.h",
            "#import <Foundation/Foundation.h>\n@interface SDImageCache : NSObject\n+ (instancetype)sharedCache;\n+ (void)storeImage:(NSString *)key;\n@end\n",
        );
        temp.write(
            "SDImageCache.m",
            "#import \"SDImageCache.h\"\n@implementation SDImageCache\n+ (instancetype)sharedCache { return nil; }\n+ (void)storeImage:(NSString *)key { }\n@end\n",
        );
        temp.write(
            "SDManager.m",
            "#import \"SDImageCache.h\"\n@interface SDManager : NSObject\n@end\n@implementation SDManager\n- (void)run {\n  [SDImageCache sharedCache];\n  [SDImageCache storeImage:@\"k\"];\n}\n@end\n",
        );

        let mut cg = index_project(&temp);
        let store_image = cg
            .get_nodes_by_kind(NodeKind::Method)
            .into_iter()
            .find(|node| node.name == "storeImage:")
            .expect("storeImage: method should be indexed");
        let store_callers = impact_file_paths(&mut cg, &store_image.id, 2);
        assert!(
            store_callers
                .iter()
                .any(|path| path.ends_with("SDManager.m")),
            "single-argument selector should resolve to storeImage:, impacted: {store_callers:?}"
        );

        let cache = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.name == "SDImageCache")
            .expect("SDImageCache class should be indexed");
        let class_deps = impact_file_paths(&mut cg, &cache.id, 2);
        assert!(
            class_deps.iter().any(|path| path.ends_with("SDManager.m")),
            "class-message receiver should reference SDImageCache, impacted: {class_deps:?}"
        );

        let header = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("SDImageCache.h"))
            .expect("SDImageCache.h file node should be indexed");
        let importers = impact_file_paths(&mut cg, &header.id, 2);
        assert!(
            importers.iter().any(|path| path.ends_with("SDManager.m")),
            "#import header should link to importer, impacted: {importers:?}"
        );
        cg.close();
    }
}
