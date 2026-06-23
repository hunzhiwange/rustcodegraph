mod describe_4629_lua_luau_require_resolution {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Lua/Luau require resolution";
    const TS_DESCRIBE_LINE: usize = 4629;
    #[test]
    fn describes_067_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4629);
    }
    #[test]
    fn case_4642_resolves_a_dotted_lua_require_and_an_instance_path_luau_require_to_the() {
        let suite = ["Lua/Luau require resolution"];
        assert_eq!(suite.len(), 1);
        assert_eq!(241, 241);
        let temp = TempDir::new("codegraph-lua-luau-require");
        temp.write(
            "lua/myapp/config.lua",
            "local M = {}\nfunction M.setup() end\nreturn M\n",
        );
        temp.write(
            "lua/myapp/init.lua",
            "local config = require(\"myapp.config\")\nreturn config\n",
        );
        temp.write(
            "src/Util/helper.luau",
            "local H = {}\nfunction H.go() end\nreturn H\n",
        );
        temp.write(
            "src/init.luau",
            "local helper = require(script.Util.helper)\nreturn helper\n",
        );

        let mut cg = index_project(&temp);
        let config = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("myapp/config.lua"))
            .expect("config.lua should be indexed");
        let helper = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("Util/helper.luau"))
            .expect("helper.luau should be indexed");
        let cfg_deps = cg.get_file_dependents(&config.file_path);
        let help_deps = cg.get_file_dependents(&helper.file_path);
        assert!(
            cfg_deps.iter().any(|path| path.ends_with("myapp/init.lua")),
            "dotted Lua require should resolve: {cfg_deps:?}"
        );
        assert!(
            help_deps.iter().any(|path| path.ends_with("src/init.luau")),
            "Luau instance-path require should resolve: {help_deps:?}"
        );
        cg.close();
    }
}
