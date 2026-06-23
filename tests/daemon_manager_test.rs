//! Interactive daemon manager tests.
//!
//! Rust port of `__tests__/daemon-manager.test.ts`.

use std::path::{Path, PathBuf};

use rustcodegraph::mcp::daemon_manager::{
    CANCEL, PickItem, PickerDeps, STOP_ALL, build_pick_items, format_uptime, run_daemon_picker,
};
use rustcodegraph::mcp::daemon_registry::{DaemonRecord, StopOutcome, StopResult};

fn rec(root: &str, pid: u32, started_at: i64) -> DaemonRecord {
    DaemonRecord {
        root: root.to_string(),
        pid,
        version: "1.0.0".to_string(),
        socket_path: format!("{root}/.rustcodegraph/daemon.sock"),
        started_at,
    }
}

mod format_uptime_suite {
    use super::*;

    #[test]
    fn formats_seconds_minutes_hours() {
        assert_eq!(format_uptime(45_000), "45s");
        assert_eq!(format_uptime(12 * 60_000), "12m");
        assert_eq!(format_uptime((3 * 60 + 5) * 60_000), "3h 5m");
    }
}

mod build_pick_items_suite {
    use super::*;

    fn values(items: &[PickItem]) -> Vec<String> {
        items.iter().map(|item| item.value.clone()).collect()
    }

    #[test]
    fn orders_newest_first_and_appends_stop_all_cancel() {
        let old = rec("/p/old", 1, 1000);
        let fresh = rec("/p/new", 2, 2000);

        let items = build_pick_items(&[old, fresh], None, 3000);

        assert_eq!(
            values(&items),
            vec![
                "/p/new".to_string(),
                "/p/old".to_string(),
                STOP_ALL.to_string(),
                CANCEL.to_string()
            ]
        );
        let hint = items[0]
            .hint
            .as_deref()
            .expect("daemon pick item should include a hint");
        assert!(hint.contains("pid 2"));
        assert!(hint.contains("Running"));
    }

    #[test]
    fn omits_stop_all_for_a_single_daemon_but_keeps_cancel() {
        let old = rec("/p/old", 1, 1000);

        assert_eq!(
            values(&build_pick_items(&[old], None, 3000)),
            vec!["/p/old".to_string(), CANCEL.to_string()]
        );
    }

    #[test]
    fn floats_the_current_project_to_the_top_auto_selected_and_labelled() {
        let old = rec("/p/old", 1, 1000);
        let fresh = rec("/p/new", 2, 2000);
        let cwd = rec("/p/cwd", 3, 500);

        let items = build_pick_items(&[old, fresh, cwd], Some(Path::new("/p/cwd")), 3000);

        assert_eq!(items[0].value, "/p/cwd");
        assert!(items[0].label.contains("(current project)"));
        assert_eq!(
            items[1..3]
                .iter()
                .map(|item| item.value.as_str())
                .collect::<Vec<_>>(),
            vec!["/p/new", "/p/old"]
        );
    }
}

mod run_daemon_picker_suite {
    use super::*;

    #[derive(Debug, Clone)]
    enum Choice {
        Value(String),
        CancelSymbol,
    }

    impl From<&str> for Choice {
        fn from(value: &str) -> Self {
            Self::Value(value.to_string())
        }
    }

    // A fake registry whose list shrinks as daemons are stopped (like the real one).
    struct Harness {
        daemons: Vec<DaemonRecord>,
        choices: Vec<Choice>,
        stopped: Vec<String>,
        notes: Vec<String>,
        done_msg: String,
        i: usize,
    }

    impl Harness {
        fn new(initial: Vec<DaemonRecord>, choices: Vec<Choice>) -> Self {
            Self {
                daemons: initial,
                choices,
                stopped: Vec::new(),
                notes: Vec::new(),
                done_msg: String::new(),
                i: 0,
            }
        }

        fn get_done(&self) -> &str {
            &self.done_msg
        }
    }

    impl PickerDeps for Harness {
        fn list(&self) -> Vec<DaemonRecord> {
            self.daemons.clone()
        }

        fn stop(&mut self, root: &str) -> StopResult {
            self.daemons.retain(|daemon| daemon.root != root);
            self.stopped.push(root.to_string());
            StopResult {
                root: root.to_string(),
                pid: Some(0),
                outcome: StopOutcome::Term,
            }
        }

        fn stop_all(&mut self) -> Vec<StopResult> {
            let all = self
                .daemons
                .iter()
                .map(|daemon| StopResult {
                    root: daemon.root.clone(),
                    pid: Some(daemon.pid),
                    outcome: StopOutcome::Term,
                })
                .collect();
            self.daemons.clear();
            self.stopped.push("ALL".to_string());
            all
        }

        fn cwd_root(&self) -> Option<PathBuf> {
            None
        }

        fn now(&self) -> i64 {
            5000
        }

        fn select(&mut self, _items: &[PickItem], _initial_value: &str) -> Option<String> {
            let choice = self.choices.get(self.i).cloned();
            self.i += 1;
            match choice {
                Some(Choice::Value(value)) => Some(value),
                Some(Choice::CancelSymbol) | None => None,
            }
        }

        fn note(&mut self, msg: &str) {
            self.notes.push(msg.to_string());
        }

        fn done(&mut self, msg: &str) {
            self.done_msg = msg.to_string();
        }
    }

    #[test]
    fn stops_the_chosen_daemon_then_re_prompts_and_exits_on_cancel() {
        let mut h = Harness::new(
            vec![rec("/p/a", 1, 1), rec("/p/b", 2, 2)],
            vec![Choice::from("/p/b"), Choice::from(CANCEL)],
        );

        run_daemon_picker(&mut h);

        assert_eq!(h.stopped, vec!["/p/b"]);
        assert!(h.get_done().contains("Cancelled"));
    }

    #[test]
    fn keeps_stopping_until_none_remain() {
        let mut h = Harness::new(
            vec![rec("/p/a", 1, 1), rec("/p/b", 2, 2)],
            vec![Choice::from("/p/a"), Choice::from("/p/b")],
        );

        run_daemon_picker(&mut h);

        assert_eq!(h.stopped, vec!["/p/a", "/p/b"]);
        assert!(h.get_done().contains("All daemons stopped"));
    }

    #[test]
    fn stop_all_stops_everything_in_one_shot() {
        let mut h = Harness::new(
            vec![rec("/p/a", 1, 1), rec("/p/b", 2, 2)],
            vec![Choice::from(STOP_ALL)],
        );

        run_daemon_picker(&mut h);

        assert_eq!(h.stopped, vec!["ALL"]);
        assert_eq!(h.get_done(), "Done.");
    }

    #[test]
    fn cancel_and_esc_ctrl_c_stop_nothing() {
        let mut h1 = Harness::new(vec![rec("/p/a", 1, 1)], vec![Choice::from(CANCEL)]);

        run_daemon_picker(&mut h1);

        assert!(h1.stopped.is_empty());
        assert!(h1.get_done().contains("Cancelled"));

        let mut h2 = Harness::new(vec![rec("/p/a", 1, 1)], vec![Choice::CancelSymbol]);

        run_daemon_picker(&mut h2);

        assert!(h2.stopped.is_empty());
        assert!(h2.get_done().contains("Cancelled"));
    }
}
