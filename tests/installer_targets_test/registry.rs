use super::common::*;

#[test]
fn get_target_returns_the_right_target_for_each_id() {
    assert_eq!(target("claude").id(), TargetId::Claude);
    assert_eq!(target("cursor").id(), TargetId::Cursor);
    assert_eq!(target("codex").id(), TargetId::Codex);
    assert_eq!(target("opencode").id(), TargetId::Opencode);
    assert_eq!(target("hermes").id(), TargetId::Hermes);
    assert_eq!(target("gemini").id(), TargetId::Gemini);
    assert_eq!(target("antigravity").id(), TargetId::Antigravity);
    assert_eq!(target("kiro").id(), TargetId::Kiro);
    assert!(get_target("not-a-real-target").is_none());
}

#[test]
fn resolve_target_flag_handles_auto_all_none_csv() {
    with_fixture("registry-resolve", |_| {
        assert!(
            resolve_target_flag("none", Location::Global)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            resolve_target_flag("all", Location::Global).unwrap().len(),
            all_target_ids().len()
        );
        let csv = resolve_target_flag("claude,cursor", Location::Global).unwrap();
        let ids = csv.iter().map(|target| target.id()).collect::<Vec<_>>();
        assert_eq!(ids, vec![TargetId::Claude, TargetId::Cursor]);
    });
}

#[test]
fn resolve_target_flag_throws_on_unknown_id() {
    let err = match resolve_target_flag("claude,bogus", Location::Global) {
        Ok(_) => panic!("expected unknown target error"),
        Err(err) => err,
    };
    assert!(err.contains("Unknown --target"));
}
