//! PPID watchdog regression test (#277).
//!
//! This is the Rust port of `__tests__/mcp-ppid-watchdog.test.ts`.

#[cfg(not(windows))]
mod mcp_ppid_watchdog_277 {
    use std::fs;
    use std::io::{BufRead, BufReader};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use regex::Regex;
    use serde::Deserialize;

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);
    const SIGKILL: i32 = 9;

    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    fn codegraph_bin() -> &'static str {
        env!("CARGO_BIN_EXE_rustcodegraph")
    }

    fn node_bin() -> String {
        std::env::var("NODE").unwrap_or_else(|_| "node".to_owned())
    }

    fn is_alive(pid: u32) -> bool {
        pid != 0 && unsafe { kill(pid as i32, 0) == 0 }
    }

    fn kill_sigkill(pid: u32) {
        if pid != 0 {
            let _ = unsafe { kill(pid as i32, SIGKILL) };
        }
    }

    fn wait_for_exit(pid: u32, timeout: Duration) -> bool {
        let start = Instant::now();
        loop {
            if !is_alive(pid) {
                return true;
            }
            if start.elapsed() > timeout {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            for _ in 0..100 {
                let unique = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system clock should be after Unix epoch")
                    .as_nanos();
                let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "{prefix}-{}-{unique}-{counter}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self { path },
                    Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(err) => panic!("failed to create temp dir {}: {err}", path.display()),
                }
            }
            panic!("failed to create a unique temp dir for {prefix}");
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    struct ProcessGuard {
        wrapper: Option<Child>,
        child_pid: Option<u32>,
        stdin_holder_pid: Option<u32>,
    }

    impl ProcessGuard {
        fn new(wrapper: Child) -> Self {
            Self {
                wrapper: Some(wrapper),
                child_pid: None,
                stdin_holder_pid: None,
            }
        }

        fn wrapper_status(&mut self) -> Option<ExitStatus> {
            self.wrapper
                .as_mut()
                .and_then(|wrapper| wrapper.try_wait().expect("wrapper try_wait should succeed"))
        }
    }

    impl Drop for ProcessGuard {
        fn drop(&mut self) {
            if let Some(wrapper) = self.wrapper.as_mut() {
                if wrapper
                    .try_wait()
                    .expect("wrapper try_wait should succeed")
                    .is_none()
                {
                    kill_sigkill(wrapper.id());
                }
                let _ = wrapper.wait();
            }

            for pid in [self.child_pid, self.stdin_holder_pid]
                .into_iter()
                .flatten()
            {
                if is_alive(pid) {
                    kill_sigkill(pid);
                }
            }
        }
    }

    #[derive(Debug, Deserialize)]
    struct Pids {
        pid: u32,
        #[serde(rename = "stdinHolderPid")]
        stdin_holder_pid: u32,
    }

    // describe.skipIf(process.platform === 'win32')('MCP PPID watchdog (#277)')
    #[test]
    fn shuts_down_when_its_parent_is_sigkilld_and_stdin_stays_open() {
        let stderr_dir = TempDir::new("cg-ppid-watchdog");
        let stderr_log = stderr_dir.path().join("codegraph.stderr.log");
        let stderr_log_json = serde_json::to_string(stderr_log.to_string_lossy().as_ref())
            .expect("stderr log path should serialize");
        let bin_json =
            serde_json::to_string(codegraph_bin()).expect("codegraph bin path should serialize");

        // The wrapper mirrors the TypeScript fixture:
        //   1. Spawn a long-lived stdin-holder whose stdout is codegraph's stdin.
        //   2. Spawn codegraph with stderr redirected to a temp file.
        //   3. Report both child PIDs, then idle until SIGKILL'd by the test.
        let wrapper_src = format!(
            r#"
const {{ spawn }} = require('child_process');
const fs = require('fs');
const stderrFd = fs.openSync({stderr_log_json}, 'a');
const stdinHolder = spawn(process.execPath, ['-e', 'setInterval(() => {{}}, 60000)'], {{
  stdio: ['ignore', 'pipe', 'ignore'],
  detached: true,
}});
stdinHolder.unref();
const child = spawn({bin_json}, ['serve', '--mcp'], {{
  stdio: [stdinHolder.stdout, 'ignore', stderrFd],
  env: {{ ...process.env, RUSTCODEGRAPH_PPID_POLL_MS: '200', RUSTCODEGRAPH_NO_DAEMON: '1' }},
  detached: true,
}});
child.unref();
setTimeout(() => {{
  process.stdout.write(JSON.stringify({{ pid: child.pid, stdinHolderPid: stdinHolder.pid }}) + '\n');
}}, 800);
setInterval(() => {{}}, 60000);
"#
        );

        let mut wrapper = Command::new(node_bin())
            .arg("-e")
            .arg(wrapper_src)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|err| panic!("failed to spawn wrapper process: {err}"));

        let stdout = wrapper
            .stdout
            .take()
            .expect("wrapper stdout should be piped");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
            let _ = tx.send(result);
        });

        let mut guard = ProcessGuard::new(wrapper);
        let line = rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|_| match guard.wrapper_status() {
                Some(status) => panic!("wrapper exited before reporting PIDs: {status}"),
                None => panic!("wrapper did not report PIDs in time"),
            })
            .unwrap_or_else(|err| panic!("failed to read wrapper PID report: {err}"));
        let pids: Pids = serde_json::from_str(line.trim()).unwrap_or_else(|err| {
            panic!("wrapper PID report should be JSON: {err}\nstdout line:\n{line}")
        });
        guard.child_pid = Some(pids.pid);
        guard.stdin_holder_pid = Some(pids.stdin_holder_pid);

        assert!(is_alive(pids.pid), "codegraph child should be alive");
        assert!(
            is_alive(pids.stdin_holder_pid),
            "stdin-holder should be alive"
        );

        // SIGKILL the wrapper. The stdin-holder keeps the pipe open, so stdin
        // close handlers cannot be what shuts codegraph down.
        let wrapper_pid = guard
            .wrapper
            .as_ref()
            .expect("wrapper should still be tracked")
            .id();
        kill_sigkill(wrapper_pid);

        let exited = wait_for_exit(pids.pid, Duration::from_secs(5));
        let stderr_content =
            fs::read_to_string(&stderr_log).unwrap_or_else(|_| "<no stderr captured>".to_owned());
        assert!(
            exited,
            "codegraph child (pid={}) did not exit within 5s after wrapper was SIGKILL'd.\nstderr:\n{}",
            pids.pid, stderr_content
        );

        let watchdog_message =
            Regex::new("Parent process exited.*shutting down").expect("regex should compile");
        assert!(
            watchdog_message.is_match(&stderr_content),
            "stderr did not show the parent-death shutdown path:\n{stderr_content}"
        );

        if is_alive(pids.stdin_holder_pid) {
            kill_sigkill(pids.stdin_holder_pid);
        }
    }
}

#[cfg(windows)]
mod mcp_ppid_watchdog_277 {
    // describe.skipIf(process.platform === 'win32')('MCP PPID watchdog (#277)')
    #[test]
    #[ignore = "process.platform === 'win32'"]
    fn shuts_down_when_its_parent_is_sigkilld_and_stdin_stays_open() {}
}
