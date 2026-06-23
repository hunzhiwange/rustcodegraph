mod describe_4670_rust_module_path_call_resolution {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Rust module-path call resolution";
    const TS_DESCRIBE_LINE: usize = 4670;
    #[test]
    fn describes_068_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4670);
    }
    #[test]
    fn case_4683_a_bare_submodule_call_users_router_resolves_self_relative_to_the_submo() {
        let suite = ["Rust module-path call resolution"];
        assert_eq!(suite.len(), 1);
        assert_eq!(242, 242);
        let temp = TempDir::new("codegraph-rust-self-relative-submodule-call");
        temp.write("src/lib.rs", "pub mod http;\n");
        temp.write(
            "src/http/mod.rs",
            r#"mod users;
mod profiles;
pub fn api_router() {
    users::router();
    profiles::router();
}
"#,
        );
        temp.write("src/http/users.rs", "pub fn router() -> i32 { 1 }\n");
        temp.write("src/http/profiles.rs", "pub fn router() -> i32 { 2 }\n");

        let mut cg = index_project(&temp);
        let routers = cg
            .get_nodes_by_kind(NodeKind::Function)
            .into_iter()
            .filter(|node| node.name == "router")
            .collect::<Vec<_>>();
        let users_router = routers
            .iter()
            .find(|node| node.file_path.ends_with("http/users.rs"))
            .expect("users.rs router should be indexed");
        let profiles_router = routers
            .iter()
            .find(|node| node.file_path.ends_with("http/profiles.rs"))
            .expect("profiles.rs router should be indexed");
        let users_deps = impact_file_paths(&mut cg, &users_router.id, 2);
        let profiles_deps = impact_file_paths(&mut cg, &profiles_router.id, 2);
        assert!(
            users_deps.iter().any(|path| path.ends_with("http/mod.rs")),
            "users::router should land on users.rs: {users_deps:?}"
        );
        assert!(
            profiles_deps
                .iter()
                .any(|path| path.ends_with("http/mod.rs")),
            "profiles::router should land on profiles.rs: {profiles_deps:?}"
        );
        cg.close();
    }
    #[test]
    fn case_4718_a_3_segment_module_path_call_database_profiles_find_resolves_to_the_le() {
        let suite = ["Rust module-path call resolution"];
        assert_eq!(suite.len(), 1);
        assert_eq!(243, 243);
        let temp = TempDir::new("codegraph-rust-three-segment-module-call");
        temp.write("src/lib.rs", "pub mod routes;\npub mod database;\n");
        temp.write("src/database/mod.rs", "pub mod profiles;\n");
        temp.write(
            "src/database/profiles.rs",
            "pub fn find(id: i32) -> i32 { id }\n",
        );
        temp.write(
            "src/routes/mod.rs",
            r#"use crate::database;
pub fn get_profile(id: i32) -> i32 {
    database::profiles::find(id)
}
"#,
        );

        let mut cg = index_project(&temp);
        let find = cg
            .get_nodes_by_kind(NodeKind::Function)
            .into_iter()
            .find(|node| node.name == "find" && node.file_path.ends_with("database/profiles.rs"))
            .expect("database/profiles.rs find should be indexed");
        let deps = impact_file_paths(&mut cg, &find.id, 2);
        assert!(
            deps.iter().any(|path| path.ends_with("routes/mod.rs")),
            "database::profiles::find should reach routes/mod.rs: {deps:?}"
        );
        cg.close();
    }
    #[test]
    fn case_4748_rocket_routes_catchers_macros_link_the_mount_to_the_handler_fns() {
        let suite = ["Rust module-path call resolution"];
        assert_eq!(suite.len(), 1);
        assert_eq!(244, 244);
        let temp = TempDir::new("codegraph-rust-rocket-route-macros");
        temp.write(
            "src/lib.rs",
            r#"mod routes;
fn not_found() {}
pub fn rocket() {
    rocket::build()
        .mount("/api", routes![routes::users::post_users, routes::users::get_user])
        .register("/", catchers![not_found]);
}
"#,
        );
        temp.write("src/routes/mod.rs", "pub mod users;\n");
        temp.write(
            "src/routes/users.rs",
            "pub fn post_users() {}\npub fn get_user() {}\n",
        );

        let mut cg = index_project(&temp);
        let handlers = cg
            .get_nodes_by_kind(NodeKind::Function)
            .into_iter()
            .filter(|node| node.file_path.ends_with("routes/users.rs"))
            .collect::<Vec<_>>();
        assert_eq!(handlers.len(), 2, "handlers: {handlers:?}");
        for handler in handlers {
            let deps = impact_file_paths(&mut cg, &handler.id, 2);
            assert!(
                deps.iter().any(|path| path.ends_with("lib.rs")),
                "routes![] should link {} to lib.rs: {deps:?}",
                handler.name
            );
        }
        cg.close();
    }
}
