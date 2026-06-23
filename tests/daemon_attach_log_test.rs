//! Daemon attach log gating (#618).
//!
//! This is the Rust port of `__tests__/daemon-attach-log.test.ts`.

use std::env;
use std::ffi::OsString;
use std::process::{Command, Stdio};

use rustcodegraph::mcp::daemon::DaemonHello;
use rustcodegraph::mcp::proxy::{LOG_ATTACH_ENV, log_attached_daemon};

const CHILD_MODE_ENV: &str = "RUSTCODEGRAPH_DAEMON_ATTACH_LOG_TEST_CHILD";

fn hello() -> DaemonHello {
    DaemonHello {
        pid: 4242,
        rustcodegraph: "9.9.9".to_string(),
        socket_path: "/tmp/cg.sock".to_string(),
        protocol: 1,
    }
}

fn run_log_attached_daemon_child(log_attach_env: Option<&str>) -> String {
    let mut command = Command::new(env::current_exe().expect("current test binary should exist"));
    command
        .arg("--exact")
        .arg("daemon_attach_log_child_process")
        .arg("--ignored")
        .arg("--nocapture")
        .env(CHILD_MODE_ENV, "1")
        .env_remove(LOG_ATTACH_ENV)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(value) = log_attach_env {
        command.env(LOG_ATTACH_ENV, value);
    }

    let output = command
        .output()
        .expect("daemon attach log child process should run");
    assert!(
        output.status.success(),
        "daemon attach log child failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stderr).expect("daemon attach stderr should be UTF-8")
}

#[test]
#[ignore = "helper invoked by daemon attach log tests"]
fn daemon_attach_log_child_process() {
    if env::var_os(CHILD_MODE_ENV).is_none() {
        return;
    }

    log_attached_daemon("/tmp/cg.sock", &hello());
}

struct EnvGuard {
    rustcodegraph: Option<OsString>,
}

impl EnvGuard {
    fn unset() -> Self {
        let guard = Self {
            rustcodegraph: env::var_os(LOG_ATTACH_ENV),
        };
        unsafe {
            env::remove_var(LOG_ATTACH_ENV);
        }
        guard
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(value) = &self.rustcodegraph {
                env::set_var(LOG_ATTACH_ENV, value);
            } else {
                env::remove_var(LOG_ATTACH_ENV);
            }
        }
    }
}

mod daemon_attach_log_gating_618 {
    use super::*;

    #[test]
    fn is_silent_by_default_no_error_undefined_noise_in_mcp_hosts() {
        let _env = EnvGuard::unset();

        let out = run_log_attached_daemon_child(None);

        assert_eq!(out, "");
    }

    #[test]
    fn logs_the_attach_line_only_when_codegraph_mcp_log_attach_1_opt_in_debug() {
        let _env = EnvGuard::unset();

        let out = run_log_attached_daemon_child(Some("1"));

        assert!(out.contains("Attached to shared daemon on /tmp/cg.sock"));
        assert!(out.contains("pid 4242"));
    }
}
