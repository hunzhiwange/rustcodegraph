#[cfg(unix)]
#[test]
fn case_004() {
    use rustcodegraph::utils::validate_project_path;
    use std::{fs, os::unix::fs::symlink};
    let root = std::env::temp_dir().join(format!("rcg-004-{}", std::process::id()));
    fs::create_dir_all(root.join("ordinary")).unwrap();
    symlink("/etc", root.join("sensitive")).unwrap();
    symlink(root.join("ordinary"), root.join("allowed")).unwrap();
    let result = std::panic::catch_unwind(|| {
        assert!(
            validate_project_path(root.join("sensitive"))
                .unwrap_or_default()
                .contains("sensitive")
        );
        assert!(
            validate_project_path(root.join("sensitive/missing-child"))
                .unwrap_or_default()
                .contains("sensitive")
        );
        assert_eq!(validate_project_path(root.join("allowed")), None);
        assert_eq!(validate_project_path(root.join("ordinary")), None);
    });
    fs::remove_dir_all(root).unwrap();
    result.unwrap();
}
