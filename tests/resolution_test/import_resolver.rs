use super::*;

#[test]
fn should_resolve_relative_import_paths() {
    let mut context = MockResolutionContext::with_files(&[
        "src/components/utils.ts",
        "src/components/utils/index.ts",
    ]);

    let result = resolve_import_path(
        "./utils",
        "src/components/Button.ts",
        Language::TypeScript,
        &mut context,
    );

    assert_eq!(result.as_deref(), Some("src/components/utils.ts"));
}

#[test]
fn should_resolve_parent_directory_imports() {
    let mut context =
        MockResolutionContext::with_files(&["src/helpers.ts", "src/helpers/index.ts"]);

    let result = resolve_import_path(
        "../helpers",
        "src/components/Button.ts",
        Language::TypeScript,
        &mut context,
    );

    assert_eq!(result.as_deref(), Some("src/helpers.ts"));
}

#[test]
fn should_extract_js_ts_import_mappings() {
    let content = r#"
import { foo } from './foo';
import bar from '../bar';
import * as utils from './utils';
import { baz, qux } from './baz';
"#;

    let mappings = extract_import_mappings("src/index.ts", content, Language::TypeScript);

    assert!(!mappings.is_empty());
    assert!(mappings.iter().any(|mapping| mapping.local_name == "foo"));
    assert!(mappings.iter().any(|mapping| mapping.local_name == "bar"));
}

#[test]
fn should_extract_rust_use_import_mappings() {
    let content = r#"
use crate::utils::format_date;
use crate::models::{User, Profile as UserProfile};
use crate::services as svc;
"#;

    let mappings = extract_import_mappings("src/main.rs", content, Language::Rust);

    assert!(mappings.iter().any(|mapping| {
        mapping.local_name == "format_date"
            && mapping.exported_name == "format_date"
            && mapping.source == "crate::utils"
    }));
    assert!(mappings.iter().any(|mapping| {
        mapping.local_name == "UserProfile"
            && mapping.exported_name == "Profile"
            && mapping.source == "crate::models"
    }));
    assert!(mappings.iter().any(|mapping| {
        mapping.local_name == "svc"
            && mapping.exported_name == "services"
            && mapping.source == "crate"
    }));
}

#[test]
fn should_extract_python_import_mappings() {
    let content = r#"
from utils import helper
from .models import User
import os
from ..services import auth_service
"#;

    let mappings = extract_import_mappings("src/main.py", content, Language::Python);

    assert!(!mappings.is_empty());
    assert!(
        mappings
            .iter()
            .any(|mapping| mapping.local_name == "helper")
    );
    assert!(mappings.iter().any(|mapping| mapping.local_name == "User"));
}
