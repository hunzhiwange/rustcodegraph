#[test]
fn generated_inventory_matches_the_typescript_suite_shape() {
    assert!(!PARITY_IGNORE_REASON.is_empty());
    assert_eq!(TS_DESCRIBE_COUNT, 117);
    assert_eq!(TS_TEST_CASE_COUNT, 372);
}
