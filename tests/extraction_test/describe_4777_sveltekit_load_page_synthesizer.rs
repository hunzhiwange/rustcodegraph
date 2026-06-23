mod describe_4777_sveltekit_load_page_synthesizer {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "SvelteKit load \u{2192} page synthesizer";
    const TS_DESCRIBE_LINE: usize = 4777;
    #[test]
    fn describes_069_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4777);
    }
    #[test]
    fn case_4790_links_a_page_svelte_to_its_own_directory_s_page_server_js_load_not_ano() {
        let suite = ["SvelteKit load \u{2192} page synthesizer"];
        assert_eq!(suite.len(), 1);
        assert_eq!(245, 245);
        let temp = TempDir::new("codegraph-sveltekit-load-page");
        temp.write(
            "src/routes/login/+page.svelte",
            "<script>export let data;</script>\n<h1>Login {data.x}</h1>\n",
        );
        temp.write(
            "src/routes/login/+page.server.js",
            "export function load() { return { x: 1 }; }\n",
        );
        temp.write(
            "src/routes/register/+page.svelte",
            "<script>export let data;</script>\n<h1>Register</h1>\n",
        );
        temp.write(
            "src/routes/register/+page.server.js",
            "export function load() { return { y: 2 }; }\n",
        );

        let mut cg = index_project(&temp);
        let login_load = cg
            .get_nodes_by_kind(NodeKind::Function)
            .into_iter()
            .find(|node| node.name == "load" && node.file_path.ends_with("login/+page.server.js"))
            .expect("login load function should be indexed");
        let impacted = cg
            .get_impact_radius(&login_load.id, 3)
            .nodes
            .into_values()
            .map(|node| node.file_path)
            .collect::<Vec<_>>();
        assert!(
            impacted
                .iter()
                .any(|path| path.ends_with("login/+page.svelte")),
            "load should link to its own page, impacted: {impacted:?}"
        );
        assert!(
            impacted
                .iter()
                .all(|path| !path.ends_with("register/+page.svelte")),
            "load should not cross routes, impacted: {impacted:?}"
        );
        cg.close();
    }
}
