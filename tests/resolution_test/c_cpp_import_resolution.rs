use super::*;

#[test]
fn should_resolve_c_include_to_header_in_same_directory() {
    let _guard = CppIncludeCacheGuard::new();
    let mut context = MockResolutionContext::with_files(&["utils.h", "main.c"]);

    let result = resolve_import_path("utils.h", "main.c", Language::C, &mut context);

    assert_eq!(result.as_deref(), Some("utils.h"));
}

#[test]
fn should_resolve_cpp_include_with_hpp_extension() {
    let _guard = CppIncludeCacheGuard::new();
    let mut context = MockResolutionContext::with_files(&["include/myclass.hpp", "src/main.cpp"])
        .with_cpp_include_dirs(&["include"]);

    let result = resolve_import_path("myclass.hpp", "src/main.cpp", Language::Cpp, &mut context);

    assert_eq!(result.as_deref(), Some("include/myclass.hpp"));
}

#[test]
fn should_resolve_include_with_subdirectory_path() {
    let _guard = CppIncludeCacheGuard::new();
    let mut context = MockResolutionContext::with_files(&["utils/helpers.h", "main.c"]);

    let result = resolve_import_path("utils/helpers.h", "main.c", Language::C, &mut context);

    assert_eq!(result.as_deref(), Some("utils/helpers.h"));
}

#[test]
fn should_resolve_include_via_include_directories() {
    let _guard = CppIncludeCacheGuard::new();
    let mut context = MockResolutionContext::with_files(&["include/myheader.h", "src/main.cpp"])
        .with_cpp_include_dirs(&["include"]);

    let result = resolve_import_path("myheader.h", "src/main.cpp", Language::Cpp, &mut context);

    assert_eq!(result.as_deref(), Some("include/myheader.h"));
}

#[test]
fn should_resolve_include_trying_multiple_extensions() {
    let _guard = CppIncludeCacheGuard::new();
    let mut context = MockResolutionContext::with_files(&["include/myclass.hpp", "src/main.cpp"])
        .with_cpp_include_dirs(&["include"]);

    let result = resolve_import_path("myclass", "src/main.cpp", Language::Cpp, &mut context);

    assert_eq!(result.as_deref(), Some("include/myclass.hpp"));
}

#[test]
fn should_return_null_for_system_headers() {
    let _guard = CppIncludeCacheGuard::new();
    let mut context = MockResolutionContext::new().with_file_exists_default(true);

    assert!(resolve_import_path("stdio.h", "main.c", Language::C, &mut context).is_none());
    assert!(resolve_import_path("vector", "main.cpp", Language::Cpp, &mut context).is_none());
    assert!(resolve_import_path("cstdio", "main.cpp", Language::Cpp, &mut context).is_none());
}

#[test]
fn should_return_null_for_single_component_third_party_paths_that_cannot_be_resolved() {
    let _guard = CppIncludeCacheGuard::new();
    let mut context = MockResolutionContext::new().with_cpp_include_dirs(&[]);

    let result = resolve_import_path("openssl/ssl.h", "main.cpp", Language::Cpp, &mut context);

    assert!(result.is_none());
}

#[test]
fn should_not_filter_project_headers_with_path_separators() {
    let _guard = CppIncludeCacheGuard::new();
    let mut context = MockResolutionContext::with_files(&["mylib/utils.h"]);

    let result = resolve_import_path("mylib/utils.h", "main.c", Language::C, &mut context);

    assert_eq!(result.as_deref(), Some("mylib/utils.h"));
}

#[test]
fn should_extract_c_cpp_import_mappings_from_include_directives() {
    let code = "#include <iostream>\n#include \"myheader.h\"\n#include \"utils/helpers.hpp\"";

    let mappings = extract_import_mappings("main.cpp", code, Language::Cpp);

    assert_eq!(mappings.len(), 3);
    assert_eq!(mappings[0].local_name, "iostream");
    assert_eq!(mappings[0].exported_name, "*");
    assert_eq!(mappings[0].source, "iostream");
    assert!(!mappings[0].is_default);
    assert!(mappings[0].is_namespace);
    assert_eq!(mappings[1].local_name, "myheader");
    assert_eq!(mappings[1].source, "myheader.h");
    assert_eq!(mappings[2].local_name, "helpers");
    assert_eq!(mappings[2].source, "utils/helpers.hpp");
}

#[test]
fn should_discover_include_directories_from_compile_commands_json() {
    let _guard = CppIncludeCacheGuard::new();
    let project = TempProject::new("codegraph-cpp-test");
    let compile_db = format!(
        r#"[{{"directory":"{}","command":"g++ -Iinclude -Isrc/lib -isystem /usr/include -c src/main.cpp","file":"src/main.cpp"}}]"#,
        project.path().display()
    );
    project.write("compile_commands.json", &compile_db);
    project.mkdir("include");
    project.mkdir("src/lib");

    let dirs = load_cpp_include_dirs(project.path());

    assert!(dirs.iter().any(|dir| dir == "include"), "dirs: {dirs:?}");
    assert!(dirs.iter().any(|dir| dir == "src/lib"), "dirs: {dirs:?}");
    assert!(
        !dirs.iter().any(|dir| dir.contains("usr")),
        "dirs: {dirs:?}"
    );
}

#[test]
fn should_fall_back_to_heuristic_include_dirs_when_no_compile_commands_json() {
    let _guard = CppIncludeCacheGuard::new();
    let project = TempProject::new("codegraph-cpp-test");
    project.write("include/types.h", "");
    project.write("src/main.cpp", "");
    project.mkdir("docs");

    let dirs = load_cpp_include_dirs(project.path());

    assert!(dirs.iter().any(|dir| dir == "include"), "dirs: {dirs:?}");
    assert!(dirs.iter().any(|dir| dir == "src"), "dirs: {dirs:?}");
    assert!(!dirs.iter().any(|dir| dir == "docs"), "dirs: {dirs:?}");
}

#[test]
fn heuristic_claims_any_top_level_dir_containing_h_files_including_objc() {
    let _guard = CppIncludeCacheGuard::new();
    let project = TempProject::new("codegraph-cpp-test");
    project.write("cppmod/shared.hpp", "");
    project.write("iosmod/View.h", "");
    project.write("iosmod/View.m", "");

    let dirs = load_cpp_include_dirs(project.path());

    assert!(dirs.iter().any(|dir| dir == "cppmod"), "dirs: {dirs:?}");
    assert!(dirs.iter().any(|dir| dir == "iosmod"), "dirs: {dirs:?}");
}

ignored_backend_test!(
    connects_include_to_the_real_header_file_via_include_dir_scan_end_to_end,
    "connects #include to the real header file via include-dir scan (end-to-end)"
);
