//! Shared MCP daemon - issue #411.
//!
//! This is the Rust port of `__tests__/mcp-daemon.test.ts`.
//!
//! The TypeScript source is an end-to-end process suite for the fully wired
//! Node daemon/socket proxy runtime. The Rust runtime accepts both framed
//! `Content-Length` MCP messages and the newline-delimited JSON-RPC used by
//! this Node-era daemon suite, so the original `it(...)` cases run as active
//! parity tests.

mod shared_mcp_daemon_issue_411 {
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use rustcodegraph::CodeGraph;
    use rustcodegraph::mcp::daemon::is_process_alive;
    use rustcodegraph::mcp::daemon_paths::get_daemon_socket_path;
    use serde_json::{Value, json};

    const BIN: &str = env!("CARGO_BIN_EXE_rustcodegraph");
    const RUST_DAEMON_PROTOCOL_COMPAT: &str = "newline JSON-RPC and Content-Length MCP";
    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct SpawnedServer {
        child: Child,
        stdout: Arc<Mutex<Vec<String>>>,
        stderr: Arc<Mutex<Vec<String>>>,
    }

    impl SpawnedServer {
        fn stdout_lines(&self) -> Vec<String> {
            self.stdout
                .lock()
                .expect("stdout lines lock should not be poisoned")
                .clone()
        }

        fn stderr_lines(&self) -> Vec<String> {
            self.stderr
                .lock()
                .expect("stderr lines lock should not be poisoned")
                .clone()
        }

        fn stderr_contains(&self, needle: &str) -> bool {
            self.stderr_lines().iter().any(|line| line.contains(needle))
        }
    }

    fn spawn_server(cwd: &Path, env: &[(&str, &str)]) -> SpawnedServer {
        let stdout_lines = Arc::new(Mutex::new(Vec::new()));
        let stderr_lines = Arc::new(Mutex::new(Vec::new()));

        let mut command = Command::new(BIN);
        command
            .args(["serve", "--mcp"])
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // #618: preserve the TypeScript harness opt-in. The Rust port also
            // wires the current RUSTCODEGRAPH_* alias so these tests can be
            // un-ignored before compatibility aliases are added to runtime code.
            .env("RUSTCODEGRAPH_MCP_LOG_ATTACH", "1")
            .env("RUSTCODEGRAPH_MCP_LOG_ATTACH", "1");

        let aliased_env = daemon_env_with_rust_aliases(env);
        for (key, value) in aliased_env {
            command.env(key, value);
        }

        let mut child = command
            .spawn()
            .unwrap_or_else(|err| panic!("failed to spawn codegraph serve --mcp: {err}"));

        let stdout = child
            .stdout
            .take()
            .expect("spawned server stdout should be piped");
        let stderr = child
            .stderr
            .take()
            .expect("spawned server stderr should be piped");

        collect_lines(stdout, Arc::clone(&stdout_lines));
        collect_lines(stderr, Arc::clone(&stderr_lines));

        SpawnedServer {
            child,
            stdout: stdout_lines,
            stderr: stderr_lines,
        }
    }

    fn daemon_env_with_rust_aliases(env: &[(&str, &str)]) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for (key, value) in env {
            out.insert((*key).to_string(), (*value).to_string());
            if let Some(alias) = rust_env_alias(key) {
                out.entry(alias.to_string())
                    .or_insert_with(|| (*value).to_string());
            }
        }
        out
    }

    fn rust_env_alias(key: &str) -> Option<&'static str> {
        match key {
            "RUSTCODEGRAPH_NO_DAEMON" => Some("RUSTCODEGRAPH_NO_DAEMON"),
            "RUSTCODEGRAPH_MCP_LOG_ATTACH" => Some("RUSTCODEGRAPH_MCP_LOG_ATTACH"),
            "RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS" => Some("RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS"),
            "RUSTCODEGRAPH_DAEMON_MAX_IDLE_MS" => Some("RUSTCODEGRAPH_DAEMON_MAX_IDLE_MS"),
            "RUSTCODEGRAPH_DAEMON_CLIENT_SWEEP_MS" => Some("RUSTCODEGRAPH_DAEMON_CLIENT_SWEEP_MS"),
            _ => None,
        }
    }

    fn collect_lines<R>(reader: R, target: Arc<Mutex<Vec<String>>>)
    where
        R: std::io::Read + Send + 'static,
    {
        thread::spawn(move || {
            let reader = BufReader::new(reader);
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                target
                    .lock()
                    .expect("line collector lock should not be poisoned")
                    .push(line);
            }
        });
    }

    fn send_message(child: &mut Child, msg: Value) {
        let Some(stdin) = child.stdin.as_mut() else {
            return;
        };
        let _ = writeln!(
            stdin,
            "{}",
            serde_json::to_string(&msg).expect("JSON-RPC message should serialize")
        );
    }

    fn send_initialize(child: &mut Child, root_uri: &str, id: i64) {
        send_message(
            child,
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "test", "version": "0.0.0" },
                    "rootUri": root_uri,
                },
            }),
        );
    }

    /// Find a JSON-RPC response with the given id (result OR error) on stdout.
    fn find_response(stdout: &[String], id: i64) -> Option<Value> {
        for line in stdout {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(parsed) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if parsed.get("id") == Some(&json!(id))
                && (parsed.get("result").is_some() || parsed.get("error").is_some())
            {
                return Some(parsed);
            }
        }
        None
    }

    fn wait_for<T, F>(mut predicate: F, timeout_ms: u64) -> T
    where
        F: FnMut() -> Option<T>,
    {
        let started = Instant::now();
        loop {
            if let Some(value) = predicate() {
                return value;
            }
            assert!(
                started.elapsed() <= Duration::from_millis(timeout_ms),
                "Timed out after {timeout_ms}ms"
            );
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn is_alive(pid: u32) -> bool {
        is_process_alive(pid)
    }

    fn read_lock_pid(root: &Path) -> Option<u32> {
        let raw = fs::read_to_string(root.join(".rustcodegraph").join("daemon.pid")).ok()?;
        let info: Value = serde_json::from_str(&raw).ok()?;
        info.get("pid")
            .and_then(Value::as_u64)
            .and_then(|pid| u32::try_from(pid).ok())
    }

    fn read_daemon_log(root: &Path) -> String {
        fs::read_to_string(root.join(".rustcodegraph").join("daemon.log")).unwrap_or_default()
    }

    fn count_listening_lines(root: &Path) -> usize {
        read_daemon_log(root)
            .lines()
            .filter(|line| line.contains("[RustCodeGraph daemon] Listening on"))
            .count()
    }

    fn kill_tree(procs: &mut [&mut Child]) {
        for proc in procs {
            let _ = proc.kill();
        }
    }

    fn wait_process_exit(pid: u32, timeout_ms: u64) -> bool {
        let started = Instant::now();
        while started.elapsed() <= Duration::from_millis(timeout_ms) {
            if !is_alive(pid) {
                return true;
            }
            thread::sleep(Duration::from_millis(25));
        }
        false
    }

    struct TempProject {
        temp_dir: PathBuf,
        real_root: PathBuf,
    }

    impl TempProject {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after Unix epoch")
                .as_nanos();
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let temp_dir = std::env::temp_dir().join(format!(
                "codegraph-mcp-daemon-{}-{unique}-{counter}",
                std::process::id(),
            ));
            fs::create_dir_all(&temp_dir).unwrap_or_else(|err| {
                panic!("failed to create temp dir {}: {err}", temp_dir.display())
            });

            let mut cg =
                CodeGraph::init_sync(&temp_dir).expect("CodeGraph should initialize fixture");
            cg.close();

            let real_root = temp_dir.canonicalize().unwrap_or_else(|_| temp_dir.clone());

            Self {
                temp_dir,
                real_root,
            }
        }

        fn temp_dir(&self) -> &Path {
            &self.temp_dir
        }

        fn real_root(&self) -> &Path {
            &self.real_root
        }

        fn root_uri(&self) -> String {
            format!("file://{}", self.temp_dir.display())
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.temp_dir);
        }
    }

    struct Harness {
        project: TempProject,
        servers: Vec<SpawnedServer>,
    }

    impl Harness {
        fn new() -> Self {
            Self {
                project: TempProject::new(),
                servers: Vec::new(),
            }
        }

        fn spawn(&mut self, env: &[(&str, &str)]) -> usize {
            let server = spawn_server(self.project.temp_dir(), env);
            self.servers.push(server);
            self.servers.len() - 1
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            for server in &mut self.servers {
                let _ = server.child.kill();
            }

            let daemon_pid = read_lock_pid(self.project.real_root());
            if let Some(pid) = daemon_pid
                && pid != std::process::id()
                && is_alive(pid)
            {
                kill_pid(pid);
            }

            thread::sleep(Duration::from_millis(50));
        }
    }

    fn kill_pid(pid: u32) {
        #[cfg(unix)]
        {
            unsafe extern "C" {
                fn kill(pid: i32, sig: i32) -> i32;
            }
            const SIGKILL: i32 = 9;
            unsafe {
                let _ = kill(pid as i32, SIGKILL);
            }
        }

        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    fn terminate_pid(pid: u32) {
        #[cfg(unix)]
        {
            unsafe extern "C" {
                fn kill(pid: i32, sig: i32) -> i32;
            }
            const SIGTERM: i32 = 15;
            unsafe {
                let _ = kill(pid as i32, SIGTERM);
            }
        }

        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    #[cfg(unix)]
    fn with_mismatched_mini_server<F>(socket_path: &Path, hello: Value, body: F)
    where
        F: FnOnce(),
    {
        use std::os::unix::net::UnixListener;

        let _ = fs::remove_file(socket_path);
        let listener = UnixListener::bind(socket_path).unwrap_or_else(|err| {
            panic!(
                "failed to bind mini daemon socket {}: {err}",
                socket_path.display()
            )
        });
        let hello_line = format!(
            "{}\n",
            serde_json::to_string(&hello).expect("mini daemon hello should serialize")
        );
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _addr)) = listener.accept() {
                let _ = stream.write_all(hello_line.as_bytes());
            }
        });

        body();
        let _ = handle.join();
        let _ = fs::remove_file(socket_path);
    }

    #[cfg(windows)]
    fn with_mismatched_mini_server<F>(socket_path: &Path, hello: Value, body: F)
    where
        F: FnOnce(),
    {
        use std::net::TcpListener;

        let listener = TcpListener::bind(rustcodegraph::mcp::daemon_paths::daemon_loopback_addr(
            socket_path,
        ))
        .unwrap_or_else(|err| {
            panic!(
                "failed to bind mini daemon loopback for {}: {err}",
                socket_path.display()
            )
        });
        let hello_line = format!(
            "{}\n",
            serde_json::to_string(&hello).expect("mini daemon hello should serialize")
        );
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _addr)) = listener.accept() {
                let _ = stream.write_all(hello_line.as_bytes());
            }
        });

        body();
        let _ = handle.join();
    }

    // TS it: "two invocations share ONE detached daemon; both attach as proxies"
    #[test]
    fn two_invocations_share_one_detached_daemon_both_attach_as_proxies() {
        let mut h = Harness::new();
        let env = [("RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS", "15000")];

        let first = h.spawn(&env);
        send_initialize(&mut h.servers[first].child, &h.project.root_uri(), 1);
        let first_resp = wait_for(
            || find_response(&h.servers[first].stdout_lines(), 1),
            10_000,
        );
        assert_eq!(first_resp["result"]["serverInfo"]["name"], "rustcodegraph");

        wait_for(
            || {
                h.servers[first]
                    .stderr_contains("Attached to shared daemon")
                    .then_some(())
            },
            8_000,
        );

        wait_for(
            || {
                h.project
                    .real_root()
                    .join(".rustcodegraph")
                    .join("daemon.pid")
                    .exists()
                    .then_some(())
            },
            8_000,
        );
        wait_for(
            || (count_listening_lines(h.project.real_root()) >= 1).then_some(()),
            8_000,
        );
        let daemon_pid =
            read_lock_pid(h.project.real_root()).expect("daemon pid should be recorded");
        assert!(is_alive(daemon_pid));

        if !cfg!(windows) {
            assert!(get_daemon_socket_path(h.project.real_root()).exists());
        }

        let second = h.spawn(&env);
        send_initialize(&mut h.servers[second].child, &h.project.root_uri(), 2);
        let second_resp = wait_for(
            || find_response(&h.servers[second].stdout_lines(), 2),
            10_000,
        );
        assert_eq!(second_resp["result"]["serverInfo"]["name"], "rustcodegraph");
        wait_for(
            || {
                h.servers[second]
                    .stderr_contains("Attached to shared daemon")
                    .then_some(())
            },
            8_000,
        );

        assert_eq!(count_listening_lines(h.project.real_root()), 1);
        assert_eq!(read_lock_pid(h.project.real_root()), Some(daemon_pid));
    }

    // TS it: "concurrent launchers converge on a single daemon (lockfile race - must-fix 1)"
    #[test]
    fn concurrent_launchers_converge_on_a_single_daemon_lockfile_race_must_fix_1() {
        let mut h = Harness::new();
        let env = [("RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS", "15000")];

        let procs = [h.spawn(&env), h.spawn(&env), h.spawn(&env)];
        for (i, index) in procs.iter().copied().enumerate() {
            send_initialize(
                &mut h.servers[index].child,
                &h.project.root_uri(),
                (i + 1) as i64,
            );
        }

        for (i, index) in procs.iter().copied().enumerate() {
            let resp = wait_for(
                || find_response(&h.servers[index].stdout_lines(), (i + 1) as i64),
                12_000,
            );
            assert_eq!(resp["result"]["serverInfo"]["name"], "rustcodegraph");
        }

        for index in procs {
            wait_for(
                || {
                    h.servers[index]
                        .stderr_contains("Attached to shared daemon")
                        .then_some(())
                },
                10_000,
            );
        }

        assert_eq!(count_listening_lines(h.project.real_root()), 1);
        let daemon_pid =
            read_lock_pid(h.project.real_root()).expect("daemon pid should be recorded");
        assert!(is_alive(daemon_pid));
    }

    // TS it: "daemon survives the first client dying; a second client keeps working (must-fix 2 / #277)"
    #[test]
    fn daemon_survives_the_first_client_dying_a_second_client_keeps_working() {
        let mut h = Harness::new();
        let env = [
            ("RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS", "30000"),
            ("RUSTCODEGRAPH_PPID_POLL_MS", "200"),
        ];

        let first = h.spawn(&env);
        send_initialize(&mut h.servers[first].child, &h.project.root_uri(), 1);
        wait_for(
            || find_response(&h.servers[first].stdout_lines(), 1),
            10_000,
        );
        wait_for(
            || read_lock_pid(h.project.real_root()).filter(|pid| *pid > 0),
            8_000,
        );
        let daemon_pid =
            read_lock_pid(h.project.real_root()).expect("daemon pid should be recorded");
        assert!(is_alive(daemon_pid));

        let second = h.spawn(&env);
        send_initialize(&mut h.servers[second].child, &h.project.root_uri(), 1);
        wait_for(
            || find_response(&h.servers[second].stdout_lines(), 1),
            10_000,
        );
        wait_for(
            || {
                h.servers[second]
                    .stderr_contains("Attached to shared daemon")
                    .then_some(())
            },
            8_000,
        );

        {
            let mut first_child = [&mut h.servers[first].child];
            kill_tree(&mut first_child);
        }

        thread::sleep(Duration::from_millis(1_500));
        assert!(is_alive(daemon_pid));

        send_message(
            &mut h.servers[second].child,
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        );
        let tools_resp = wait_for(
            || find_response(&h.servers[second].stdout_lines(), 2),
            10_000,
        );
        assert!(tools_resp["result"]["tools"].is_array());
        assert!(
            !tools_resp["result"]["tools"]
                .as_array()
                .expect("tools should be an array")
                .is_empty()
        );
    }

    // TS it: "RUSTCODEGRAPH_NO_DAEMON=1 keeps each process independent (no socket/pidfile)"
    #[test]
    fn codegraph_no_daemon_1_keeps_each_process_independent_no_socket_pidfile() {
        let mut h = Harness::new();
        let env = [("RUSTCODEGRAPH_NO_DAEMON", "1")];

        let first = h.spawn(&env);
        send_initialize(&mut h.servers[first].child, &h.project.root_uri(), 1);
        wait_for(
            || find_response(&h.servers[first].stdout_lines(), 1),
            10_000,
        );

        assert!(!h.servers[first].stderr_contains("Attached to shared daemon"));
        assert!(
            !h.project
                .real_root()
                .join(".rustcodegraph")
                .join("daemon.pid")
                .exists()
        );
        assert!(
            !h.project
                .real_root()
                .join(".rustcodegraph")
                .join("daemon.log")
                .exists()
        );
    }

    // TS it: "clears a stale (dead-pid) lockfile and a fresh daemon takes over"
    #[test]
    fn clears_a_stale_dead_pid_lockfile_and_a_fresh_daemon_takes_over() {
        let mut h = Harness::new();
        fs::write(
            h.project
                .real_root()
                .join(".rustcodegraph")
                .join("daemon.pid"),
            serde_json::to_string_pretty(&json!({
                "pid": 999_999u32,
                "version": "0.0.0-fake",
                "socketPath": get_daemon_socket_path(h.project.real_root()).to_string_lossy(),
                "startedAt": now_ms() - 1_000,
            }))
            .expect("stale pidfile should serialize"),
        )
        .expect("stale pidfile should be written");

        let env = [("RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS", "15000")];
        let server = h.spawn(&env);
        send_initialize(&mut h.servers[server].child, &h.project.root_uri(), 1);
        let resp = wait_for(
            || find_response(&h.servers[server].stdout_lines(), 1),
            10_000,
        );
        assert_eq!(resp["result"]["serverInfo"]["name"], "rustcodegraph");
        wait_for(
            || (count_listening_lines(h.project.real_root()) >= 1).then_some(()),
            10_000,
        );

        let live_pid = read_lock_pid(h.project.real_root()).expect("daemon pid should be recorded");
        assert_ne!(live_pid, 999_999);
        assert!(is_alive(live_pid));
    }

    // TS it: "proxy falls back to direct mode on a daemon version mismatch"
    #[test]
    fn proxy_falls_back_to_direct_mode_on_a_daemon_version_mismatch() {
        let mut h = Harness::new();
        let sock_path = get_daemon_socket_path(h.project.real_root());
        fs::write(
            h.project
                .real_root()
                .join(".rustcodegraph")
                .join("daemon.pid"),
            serde_json::to_string_pretty(&json!({
                "pid": std::process::id(),
                "version": "0.0.0-mismatch",
                "socketPath": sock_path.to_string_lossy(),
                "startedAt": now_ms(),
            }))
            .expect("version-mismatch pidfile should serialize"),
        )
        .expect("version-mismatch pidfile should be written");

        with_mismatched_mini_server(
            &sock_path,
            json!({
                "rustcodegraph": "0.0.0-mismatch",
                "pid": 1,
                "socketPath": sock_path.to_string_lossy(),
                "protocol": 1,
            }),
            || {
                let server = h.spawn(&[]);
                send_initialize(&mut h.servers[server].child, &h.project.root_uri(), 1);
                let resp = wait_for(
                    || find_response(&h.servers[server].stdout_lines(), 1),
                    10_000,
                );
                assert_eq!(resp["result"]["serverInfo"]["name"], "rustcodegraph");
                wait_for(
                    || {
                        h.servers[server]
                            .stderr_contains("serving this session in-process")
                            .then_some(())
                    },
                    6_000,
                );
            },
        );
    }

    // TS it: "exits on the inactivity backstop even while a client stays connected (#692)"
    #[test]
    fn exits_on_the_inactivity_backstop_even_while_a_client_stays_connected_692() {
        let mut h = Harness::new();
        let env = [
            ("RUSTCODEGRAPH_DAEMON_MAX_IDLE_MS", "1500"),
            ("RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS", "60000"),
        ];
        let server = h.spawn(&env);
        send_initialize(&mut h.servers[server].child, &h.project.root_uri(), 1);
        wait_for(
            || find_response(&h.servers[server].stdout_lines(), 1),
            10_000,
        );
        wait_for(
            || read_lock_pid(h.project.real_root()).filter(|pid| *pid > 0),
            8_000,
        );
        let daemon_pid =
            read_lock_pid(h.project.real_root()).expect("daemon pid should be recorded");
        assert!(is_alive(daemon_pid));

        assert!(wait_process_exit(daemon_pid, 12_000));
        assert!(read_daemon_log(h.project.real_root()).contains("inactivity backstop"));
        assert!(
            !h.project
                .real_root()
                .join(".rustcodegraph")
                .join("daemon.pid")
                .exists()
        );
    }

    // TS it: "daemon idle-times-out after the last client disconnects"
    #[test]
    fn daemon_idle_times_out_after_the_last_client_disconnects() {
        let mut h = Harness::new();
        let env = [
            ("RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS", "800"),
            ("RUSTCODEGRAPH_PPID_POLL_MS", "200"),
        ];
        let server = h.spawn(&env);
        send_initialize(&mut h.servers[server].child, &h.project.root_uri(), 1);
        wait_for(
            || find_response(&h.servers[server].stdout_lines(), 1),
            10_000,
        );
        wait_for(
            || read_lock_pid(h.project.real_root()).filter(|pid| *pid > 0),
            8_000,
        );
        let daemon_pid =
            read_lock_pid(h.project.real_root()).expect("daemon pid should be recorded");

        if let Some(stdin) = h.servers[server].child.stdin.as_mut() {
            let _ = stdin.flush();
        }
        let _ = h.servers[server].child.stdin.take();

        assert!(wait_process_exit(daemon_pid, 10_000));
        assert!(
            !h.project
                .real_root()
                .join(".rustcodegraph")
                .join("daemon.pid")
                .exists()
        );
    }

    // TS it: "proxy survives the daemon dying mid-session and keeps serving (#662)"
    #[test]
    fn proxy_survives_the_daemon_dying_mid_session_and_keeps_serving_662() {
        let mut h = Harness::new();
        let env = [
            ("RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS", "30000"),
            ("RUSTCODEGRAPH_PPID_POLL_MS", "5000"),
        ];
        let server = h.spawn(&env);
        send_initialize(&mut h.servers[server].child, &h.project.root_uri(), 1);
        wait_for(
            || find_response(&h.servers[server].stdout_lines(), 1),
            10_000,
        );
        wait_for(
            || {
                h.servers[server]
                    .stderr_contains("Attached to shared daemon")
                    .then_some(())
            },
            8_000,
        );
        wait_for(
            || read_lock_pid(h.project.real_root()).filter(|pid| *pid > 0),
            8_000,
        );
        let daemon_pid =
            read_lock_pid(h.project.real_root()).expect("daemon pid should be recorded");

        send_message(
            &mut h.servers[server].child,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "codegraph_status", "arguments": {} },
            }),
        );
        wait_for(
            || find_response(&h.servers[server].stdout_lines(), 2),
            10_000,
        );

        terminate_pid(daemon_pid);
        assert!(wait_process_exit(daemon_pid, 8_000));

        assert!(h.servers[server].child.id() > 0);
        wait_for(
            || {
                h.servers[server]
                    .stderr_contains("serving this session in-process")
                    .then_some(())
            },
            8_000,
        );
        send_message(
            &mut h.servers[server].child,
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": { "name": "codegraph_status", "arguments": {} },
            }),
        );
        let resp = wait_for(
            || find_response(&h.servers[server].stdout_lines(), 3),
            15_000,
        );
        assert!(resp.get("result").is_some() || resp.get("error").is_some());
        assert!(h.servers[server].child.id() > 0);
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_millis() as i64
    }

    #[test]
    fn records_the_current_daemon_protocol_compatibility() {
        assert_eq!(
            RUST_DAEMON_PROTOCOL_COMPAT,
            "newline JSON-RPC and Content-Length MCP"
        );
    }
}
