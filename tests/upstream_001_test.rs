const SCRIPT: &str = include_str!("../scripts/agent-eval/ab-new-vs-baseline.sh");

#[test]
fn case_001_snapshot_builds() {
    assert!(SCRIPT.contains("git -C \"$ENGINE\" archive"));
    assert!(SCRIPT.contains("CARGO_TARGET_DIR"));
    assert!(SCRIPT.contains("slot-$n.lock"));
}
#[test]
fn case_001_no_checkout_mutation() {
    assert!(!SCRIPT.contains(" checkout "));
}
#[test]
fn case_001_no_global_process_cleanup() {
    assert!(!SCRIPT.contains("pkill"));
    assert!(SCRIPT.contains("wait \"$DAEMON_PID\""));
}

#[cfg(unix)]
#[test]
fn case_001_failure_cleanup_preserves_unrelated_process() {
    let start = SCRIPT.find("DAEMON_PID=\"\"").unwrap();
    let end = SCRIPT.find("build_ref() {").unwrap();
    let functions = &SCRIPT[start..end];
    let program = format!(
        "{functions}\nsleep 30 &\nunrelated=$!\nsleep 30 &\nDAEMON_PID=$!\nowned=$DAEMON_PID\ncleanup\nkill -0 $unrelated || exit 1\nif kill -0 $owned 2>/dev/null; then kill $owned; kill -KILL $unrelated; exit 2; fi\nkill -KILL $unrelated\nwait $unrelated 2>/dev/null || true\n"
    );
    assert!(
        std::process::Command::new("bash")
            .args(["-c", &program])
            .status()
            .unwrap()
            .success()
    );
}
