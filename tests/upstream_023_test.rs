#[path = "../src/mcp/server_instructions.rs"]
mod instructions;

#[test]
fn case_023_guides_budget_and_repeated_source() {
    let text = instructions::SERVER_INSTRUCTIONS;
    assert!(text.contains("output budget"));
    assert!(text.contains("previously returned source"));
    assert!(text.contains("already Read"));
}

#[test]
fn case_023_follow_up_uses_existing_explore_input() {
    let text = instructions::SERVER_INSTRUCTIONS;
    assert!(text.contains("more specific symbol or file names"));
    assert!(text.contains("not a quota"));
}

#[test]
fn case_023_inactive_workspace_does_not_offer_budget_guidance() {
    let text = instructions::SERVER_INSTRUCTIONS_UNINDEXED;
    assert!(!text.contains("output budget"));
    assert!(!text.contains("previously returned source"));
}
