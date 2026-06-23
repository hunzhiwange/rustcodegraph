//! Unit coverage for the daemon-side client-liveness primitives (#692, Layer 2).
//!
//! This is the Rust port of `__tests__/daemon-client-liveness.test.ts`.
//!
//! These back the daemon's defense against a phantom client -- one whose process
//! died without the socket ever signalling close (a Windows named-pipe hazard).
//! The wire parsing and the liveness decision are pure, so they're tested here;
//! the full handshake + sweep is exercised end-to-end in `mcp-daemon.test.ts`.

use std::path::PathBuf;

use rustcodegraph::mcp::daemon::{
    ClientPeerPids, Daemon, DaemonClientHandle, DaemonOptions, parse_client_hello_line,
    peer_is_dead,
};

mod parse_client_hello_line_suite {
    use super::*;

    #[test]
    fn parses_a_well_formed_client_hello() {
        assert_eq!(
            parse_client_hello_line(r#"{"rustcodegraph_client":1,"pid":1234,"hostPid":56}"#),
            Some(ClientPeerPids {
                pid: Some(1234),
                host_pid: Some(56),
            })
        );
    }

    #[test]
    fn accepts_a_null_host_pid_and_a_missing_host_pid() {
        assert_eq!(
            parse_client_hello_line(r#"{"rustcodegraph_client":1,"pid":1234,"hostPid":null}"#),
            Some(ClientPeerPids {
                pid: Some(1234),
                host_pid: None,
            })
        );
        assert_eq!(
            parse_client_hello_line(r#"{"rustcodegraph_client":1,"pid":1234}"#),
            Some(ClientPeerPids {
                pid: Some(1234),
                host_pid: None,
            })
        );
    }

    #[test]
    fn returns_null_for_a_json_rpc_message_no_marker_so_it_is_treated_as_data() {
        assert_eq!(
            parse_client_hello_line(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#),
            None
        );
    }

    #[test]
    fn rejects_a_wrong_typed_marker_a_non_numeric_pid_and_a_non_integer_marker() {
        assert_eq!(
            parse_client_hello_line(r#"{"rustcodegraph_client":true,"pid":1}"#),
            None
        );
        assert_eq!(
            parse_client_hello_line(r#"{"rustcodegraph_client":2,"pid":1}"#),
            None
        );
        assert_eq!(
            parse_client_hello_line(r#"{"rustcodegraph_client":1,"pid":"1"}"#),
            None
        );
    }

    #[test]
    fn returns_null_for_invalid_empty_non_object_json() {
        assert_eq!(parse_client_hello_line("not json"), None);
        assert_eq!(parse_client_hello_line(""), None);
        assert_eq!(parse_client_hello_line("42"), None);
        assert_eq!(parse_client_hello_line("null"), None);
    }
}

mod peer_is_dead_suite {
    use super::*;

    fn alive_all(_pid: u32) -> bool {
        true
    }

    fn dead_all(_pid: u32) -> bool {
        false
    }

    fn dead_only<const N: usize>(pids: [u32; N]) -> impl Fn(u32) -> bool {
        move |pid| !pids.contains(&pid)
    }

    #[test]
    fn never_reaps_a_client_with_an_unknown_pid_no_client_hello() {
        assert!(!peer_is_dead(
            ClientPeerPids {
                pid: None,
                host_pid: None,
            },
            dead_all
        ));
        assert!(!peer_is_dead(
            ClientPeerPids {
                pid: None,
                host_pid: Some(99),
            },
            dead_all
        ));
    }

    #[test]
    fn keeps_a_client_whose_proxy_is_alive() {
        assert!(!peer_is_dead(
            ClientPeerPids {
                pid: Some(100),
                host_pid: None,
            },
            alive_all
        ));
    }

    #[test]
    fn reaps_a_client_whose_proxy_process_is_gone() {
        assert!(peer_is_dead(
            ClientPeerPids {
                pid: Some(100),
                host_pid: None,
            },
            dead_only([100])
        ));
    }

    #[test]
    fn reaps_when_the_proxy_is_alive_but_its_host_is_gone() {
        // proxy 100 alive, host 42 dead
        assert!(peer_is_dead(
            ClientPeerPids {
                pid: Some(100),
                host_pid: Some(42),
            },
            dead_only([42])
        ));
    }

    #[test]
    fn keeps_a_client_when_both_proxy_and_host_are_alive() {
        assert!(!peer_is_dead(
            ClientPeerPids {
                pid: Some(100),
                host_pid: Some(42),
            },
            alive_all
        ));
    }
}

mod daemon_reap_dead_clients_suite {
    use super::*;

    // Construct with idleTimeoutMs:0 so dropping the last client doesn't arm a real
    // idle timer. The constructor opens no sockets/DB, so this stays a fast unit test.
    fn make_daemon() -> Daemon {
        Daemon::new(
            PathBuf::from("/tmp/codegraph-reap-unit-test"),
            Some(DaemonOptions {
                idle_timeout_ms: Some(0),
                max_idle_ms: None,
            }),
        )
    }

    fn fake_session(
        daemon: &mut Daemon,
        pid: Option<u32>,
        host_pid: Option<u32>,
    ) -> DaemonClientHandle {
        daemon.add_client_for_liveness_test(ClientPeerPids { pid, host_pid })
    }

    #[test]
    fn drops_clients_with_a_dead_peer_and_leaves_live_ones_attached() {
        let mut d = make_daemon();
        let dead = fake_session(&mut d, Some(111), None);
        let live = fake_session(&mut d, Some(222), None);

        let reaped = d.reap_dead_clients(|pid| pid != 111); // 111 dead, 222 alive

        assert_eq!(reaped, 1);
        assert!(d.client_was_stopped_for_liveness_test(dead));
        assert!(!d.has_client_for_liveness_test(dead));
        assert_eq!(d.client_peer_for_liveness_test(dead), None); // peer record cleaned up too
        assert!(d.has_client_for_liveness_test(live));
    }

    #[test]
    fn never_reaps_a_client_with_an_unknown_pid_no_client_hello() {
        let mut d = make_daemon();
        let s = fake_session(&mut d, None, None);

        assert_eq!(d.reap_dead_clients(|_| false), 0); // everything "dead", but pid unknown
        assert!(d.has_client_for_liveness_test(s));
    }

    #[test]
    fn reaps_a_client_whose_host_pid_is_gone_even_if_its_proxy_pid_is_alive() {
        let mut d = make_daemon();
        let s = fake_session(&mut d, Some(100), Some(42));

        assert_eq!(d.reap_dead_clients(|pid| pid != 42), 1); // proxy 100 alive, host 42 dead
        assert!(!d.has_client_for_liveness_test(s));
    }
}
