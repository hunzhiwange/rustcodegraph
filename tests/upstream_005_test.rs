use rustcodegraph::utils::validate_project_path;

#[test]
fn case_005_normal_directory() {
    assert_eq!(
        validate_project_path(std::env::current_dir().unwrap()),
        None
    );
}

#[test]
#[cfg(any(target_os = "macos", windows))]
fn case_005_credentials_case() {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap();
    for tail in [".SSH", ".AWS/missing-child", ".CONFIG"] {
        let result = validate_project_path(std::path::PathBuf::from(&home).join(tail)).unwrap();
        assert!(result.contains("sensitive directory"), "{result}");
    }
}

#[test]
#[cfg(windows)]
fn case_005_verbatim_system() {
    for path in [
        r"\\?\C:\Windows",
        r"\\?\C:\WINDOWS\System32",
        r"\\.\C:\Windows",
    ] {
        assert!(
            validate_project_path(path)
                .unwrap()
                .contains("sensitive system directory")
        );
    }
}

#[test]
#[cfg(target_os = "linux")]
fn case_005_linux_preserves_case() {
    assert!(
        !validate_project_path("/ETC")
            .unwrap()
            .contains("sensitive system directory")
    );
}
