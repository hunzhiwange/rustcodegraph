//! Unit coverage for the PPID-watchdog decision logic (#277, #692).
//!
//! This is the Rust port of `__tests__/ppid-watchdog.test.ts`.

use rustcodegraph::mcp::ppid_watchdog::{SupervisionState, supervision_lost_reason};

fn alive(_: u32) -> bool {
    true
}

fn dead(_: u32) -> bool {
    false
}

/// Alive for everyone except the listed pids.
fn dead_only(pids: &[u32]) -> impl Fn(u32) -> bool + '_ {
    move |pid| !pids.contains(&pid)
}

mod supervision_lost_reason {
    use super::*;

    mod posix_parent_death_reparents_ppid_changes {
        use super::*;

        #[test]
        fn returns_null_while_the_parent_is_unchanged() {
            assert_eq!(
                supervision_lost_reason(SupervisionState {
                    original_ppid: 100,
                    current_ppid: 100,
                    host_ppid: None,
                    is_alive: alive,
                    platform: Some("linux"),
                }),
                None
            );
        }

        #[test]
        fn detects_a_reparent_ppid_divergence_as_the_death_signal() {
            let reason = supervision_lost_reason(SupervisionState {
                original_ppid: 100,
                current_ppid: 1, // reparented to init
                host_ppid: None,
                is_alive: alive,
                platform: Some("linux"),
            });
            assert_eq!(reason.as_deref(), Some("ppid 100 -> 1"));
        }

        #[test]
        fn does_not_use_liveness_on_posix_a_dead_original_ppid_is_not_orphaning() {
            // A double-forked grandparent can die while we stay correctly
            // parented. POSIX must rely on the change-check only, or it would
            // false-positive.
            assert_eq!(
                supervision_lost_reason(SupervisionState {
                    original_ppid: 100,
                    current_ppid: 100,
                    host_ppid: None,
                    is_alive: dead,
                    platform: Some("linux"),
                }),
                None
            );
        }
    }

    mod windows_ppid_is_stable_across_parent_death_poll_liveness {
        use super::*;

        #[test]
        fn returns_null_while_the_original_parent_is_still_alive() {
            assert_eq!(
                supervision_lost_reason(SupervisionState {
                    original_ppid: 100,
                    current_ppid: 100,
                    host_ppid: None,
                    is_alive: alive,
                    platform: Some("windows"),
                }),
                None
            );
        }

        #[test]
        fn detects_parent_death_by_liveness_even_though_ppid_is_unchanged_the_692_fix() {
            let reason = supervision_lost_reason(SupervisionState {
                original_ppid: 100,
                current_ppid: 100, // Windows never reparents
                host_ppid: None,
                is_alive: dead_only(&[100]),
                platform: Some("windows"),
            });
            assert_eq!(reason.as_deref(), Some("parent pid 100 exited"));
        }

        #[test]
        fn ignores_pid_0_1_never_a_real_windows_parent_must_not_trigger_shutdown() {
            for ppid in [0, 1] {
                assert_eq!(
                    supervision_lost_reason(SupervisionState {
                        original_ppid: ppid,
                        current_ppid: ppid,
                        host_ppid: None,
                        is_alive: dead,
                        platform: Some("windows"),
                    }),
                    None
                );
            }
        }
    }

    mod threaded_host_pid_reached_past_an_intermediate_launcher_shim {
        use super::*;

        #[test]
        fn shuts_down_when_the_host_pid_is_gone_on_either_platform() {
            for platform in ["linux", "windows"] {
                let reason = supervision_lost_reason(SupervisionState {
                    original_ppid: 100,
                    current_ppid: 100,
                    host_ppid: Some(42),
                    is_alive: dead_only(&[42]), // shim 100 alive, host 42 dead
                    platform: Some(platform),
                });
                assert_eq!(reason.as_deref(), Some("host pid 42 exited"));
            }
        }

        #[test]
        fn stays_supervised_while_the_host_pid_is_alive() {
            assert_eq!(
                supervision_lost_reason(SupervisionState {
                    original_ppid: 100,
                    current_ppid: 100,
                    host_ppid: Some(42),
                    is_alive: alive,
                    platform: Some("linux"),
                }),
                None
            );
        }
    }

    mod signal_precedence {
        use super::*;

        #[test]
        fn reports_the_ppid_change_ahead_of_a_host_gone_reason() {
            let reason = supervision_lost_reason(SupervisionState {
                original_ppid: 100,
                current_ppid: 1,
                host_ppid: Some(42),
                is_alive: dead,
                platform: Some("linux"),
            });
            assert_eq!(reason.as_deref(), Some("ppid 100 -> 1"));
        }
    }
}
