use super::*;

#[test]
fn should_detect_react_framework() {
    let mut context = MockResolutionContext::new()
        .with_file_contents(&[("package.json", r#"{"dependencies":{"react":"^18.0.0"}}"#)])
        .with_all_files(&["package.json", "src/App.tsx"])
        .with_project_root("/test");

    let frameworks = detect_frameworks(&mut context);
    assert!(frameworks.iter().any(|resolver| resolver.name() == "react"));
}

#[test]
fn should_detect_express_framework() {
    let mut context = MockResolutionContext::new()
        .with_file_contents(&[("package.json", r#"{"dependencies":{"express":"^4.18.0"}}"#)])
        .with_all_files(&["package.json", "src/app.js"])
        .with_project_root("/test");

    let frameworks = detect_frameworks(&mut context);
    assert!(
        frameworks
            .iter()
            .any(|resolver| resolver.name() == "express")
    );
}

#[test]
fn should_detect_laravel_framework() {
    let mut context = MockResolutionContext::with_files(&["artisan", "app/Http/Kernel.php"])
        .with_project_root("/test");

    let frameworks = detect_frameworks(&mut context);
    assert!(
        frameworks
            .iter()
            .any(|resolver| resolver.name() == "laravel")
    );
}

#[test]
fn should_return_all_framework_resolvers() {
    let resolvers = get_all_framework_resolvers();

    assert!(!resolvers.is_empty());
    assert!(resolvers.iter().any(|resolver| resolver.name() == "react"));
    assert!(
        resolvers
            .iter()
            .any(|resolver| resolver.name() == "express")
    );
    assert!(
        resolvers
            .iter()
            .any(|resolver| resolver.name() == "laravel")
    );
}
