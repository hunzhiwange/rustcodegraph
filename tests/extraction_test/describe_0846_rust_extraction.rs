mod describe_0846_rust_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Rust Extraction";
    const TS_DESCRIBE_LINE: usize = 846;
    #[test]
    fn describes_010_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 846);
    }
    #[test]
    fn case_0847_should_extract_function_declarations() {
        let suite = ["Rust Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(57, 57);
        let code = r#"
pub fn process_data(input: &str) -> Result<Output, Error> {
    Ok(Output::new())
}
"#;
        let result = extract("lib.rs", code);
        let func_node = find_node(&result, NodeKind::Function, "process_data")
            .expect("process_data function should be extracted");
        assert_eq!(func_node.visibility, Some(Visibility::Public));
    }
    #[test]
    fn case_0862_should_extract_struct_declarations() {
        let suite = ["Rust Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(58, 58);
        let code = r#"
pub struct User {
    pub id: String,
    pub name: String,
    email: String,
}
"#;
        let result = extract("models.rs", code);
        let struct_node =
            find_node(&result, NodeKind::Struct, "User").expect("User struct should be extracted");
        assert_eq!(struct_node.language, Language::Rust);
    }
    #[test]
    fn case_0877_should_extract_trait_declarations() {
        let suite = ["Rust Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(59, 59);
        let code = r#"
pub trait Repository {
    fn find(&self, id: &str) -> Option<Entity>;
    fn save(&mut self, entity: Entity) -> Result<(), Error>;
}
"#;
        let result = extract("traits.rs", code);
        let trait_node = find_node(&result, NodeKind::Trait, "Repository")
            .expect("Repository trait should be extracted");
        assert_eq!(trait_node.language, Language::Rust);
    }
    #[test]
    fn case_0891_should_extract_impl_trait_for_type_as_implements_edges() {
        let suite = ["Rust Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(60, 60);
        let code = r#"
pub struct MyCache {}

pub trait Cache {
    fn get(&self, key: &str) -> Option<String>;
}

impl Cache for MyCache {
    fn get(&self, key: &str) -> Option<String> {
        None
    }
}
"#;
        let result = extract("cache.rs", code);
        let my_cache = find_node(&result, NodeKind::Struct, "MyCache")
            .expect("MyCache struct should be extracted");
        assert!(result.unresolved_references.iter().any(|reference| {
            reference.reference_kind == ReferenceKind::Implements
                && reference.reference_name == "Cache"
                && reference.from_node_id == my_cache.id
        }));
    }
    #[test]
    fn case_0919_should_extract_trait_supertraits_as_extends_references() {
        let suite = ["Rust Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(61, 61);
        let code = r#"
pub trait Display {}

pub trait Error: Display {
    fn description(&self) -> &str;
}
"#;
        let result = extract("error.rs", code);
        let error_trait =
            find_node(&result, NodeKind::Trait, "Error").expect("Error trait should be extracted");
        assert!(result.unresolved_references.iter().any(|reference| {
            reference.reference_kind == ReferenceKind::Extends
                && reference.reference_name == "Display"
                && reference.from_node_id == error_trait.id
        }));
    }
    #[test]
    fn case_0939_should_not_create_implements_edges_for_plain_impl_blocks() {
        let suite = ["Rust Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(62, 62);
        let code = r#"
pub struct Counter {
    count: u32,
}

impl Counter {
    pub fn new() -> Counter {
        Counter { count: 0 }
    }
    pub fn increment(&mut self) {
        self.count += 1;
    }
}
"#;
        let result = extract("counter.rs", code);
        assert!(
            result
                .unresolved_references
                .iter()
                .all(|reference| reference.reference_kind != ReferenceKind::Implements),
            "references: {:?}",
            result.unresolved_references
        );
    }
}
