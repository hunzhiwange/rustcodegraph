mod describe_4820_nuxt_nested_auto_imported_component_resolution {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Nuxt nested auto-imported component resolution";
    const TS_DESCRIBE_LINE: usize = 4820;
    #[test]
    fn describes_070_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4820);
    }
    #[test]
    fn case_4833_links_a_mediacard_usage_to_components_media_card_vue_nuxt_dir_prefixed() {
        let suite = ["Nuxt nested auto-imported component resolution"];
        assert_eq!(suite.len(), 1);
        assert_eq!(246, 246);
        let temp = TempDir::new("codegraph-nuxt-nested-auto-import");
        temp.write(
            "components/media/Card.vue",
            "<template><div>card</div></template>\n<script setup>defineProps(['item'])</script>\n",
        );
        temp.write(
            "components/Grid.vue",
            "<template>\n  <div><MediaCard :item=\"i\" /></div>\n</template>\n<script setup>const i = {}</script>\n",
        );

        let mut cg = index_project(&temp);
        let card = cg
            .get_nodes_by_kind(NodeKind::Component)
            .into_iter()
            .find(|node| node.file_path.ends_with("components/media/Card.vue"))
            .expect("media/Card.vue component should be indexed");
        let impacted = cg
            .get_impact_radius(&card.id, 2)
            .nodes
            .into_values()
            .map(|node| node.file_path)
            .collect::<Vec<_>>();
        assert!(
            impacted
                .iter()
                .any(|path| path.ends_with("components/Grid.vue")),
            "<MediaCard> should link Grid to media/Card.vue, impacted: {impacted:?}"
        );
        cg.close();
    }
}
