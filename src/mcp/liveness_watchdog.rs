//! Main-thread liveness watchdog translated from `liveness-watchdog.ts`.
//!
//! watchdog 是一个独立 Node 子进程：主线程定期向它 stdin 打心跳，若主进程
//! 卡死超过超时，它会杀掉父进程，让宿主下一次请求能启动新 server。

use std::env;
use std::ffi::OsString;
use std::fmt;
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub const DEFAULT_WATCHDOG_TIMEOUT_MS: u64 = 60_000;

pub const CHILD_SOURCE: &str = r#"
const fs = require('fs');
const parentPid = Number(process.argv[1]);
const timeoutMs = Number(process.argv[2]);
const secs = Math.round(timeoutMs / 1000);
const MSG = Buffer.from('[RustCodeGraph] Main thread unresponsive for ~' + secs + 's - killing the wedged process so a fresh one can start (#850). Disable with RUSTCODEGRAPH_NO_WATCHDOG=1.\n');
function kill() {
  try { fs.writeSync(2, MSG); } catch (e) {}
  try { process.kill(parentPid, 'SIGKILL'); } catch (e) {}
  process.exit(0);
}
let timer = setTimeout(kill, timeoutMs);
process.stdin.on('data', () => { clearTimeout(timer); timer = setTimeout(kill, timeoutMs); });
process.stdin.on('end', () => process.exit(0));
process.stdin.on('error', () => process.exit(0));
process.stdin.resume();
"#;

pub trait WatchdogHandle {
    fn stop(&self);
}

#[derive(Clone)]
pub struct MainThreadWatchdogHandle {
    inner: Arc<WatchdogInner>,
}

struct WatchdogInner {
    stopped: AtomicBool,
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
    heartbeat: Mutex<Option<JoinHandle<()>>>,
}

impl fmt::Debug for MainThreadWatchdogHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MainThreadWatchdogHandle")
            .field("stopped", &self.stopped())
            .finish_non_exhaustive()
    }
}

impl MainThreadWatchdogHandle {
    fn new(child: Child, stdin: ChildStdin) -> Self {
        Self {
            inner: Arc::new(WatchdogInner {
                stopped: AtomicBool::new(false),
                child: Mutex::new(Some(child)),
                stdin: Mutex::new(Some(stdin)),
                heartbeat: Mutex::new(None),
            }),
        }
    }

    pub fn stopped(&self) -> bool {
        self.inner.stopped.load(Ordering::SeqCst)
    }

    pub fn heartbeat(&self) {
        // 心跳是 best-effort；写失败说明子进程已退出或管道断开，主流程不应 panic。
        if self.stopped() {
            return;
        }

        let mut stdin = self
            .inner
            .stdin
            .lock()
            .expect("watchdog stdin lock should not be poisoned");
        if let Some(stdin) = stdin.as_mut() {
            let _ = stdin.write_all(b"\n");
            let _ = stdin.flush();
        }
    }

    pub fn stop(&self) {
        if self.inner.stopped.swap(true, Ordering::SeqCst) {
            return;
        }

        if let Some(join) = self
            .inner
            .heartbeat
            .lock()
            .expect("watchdog heartbeat lock should not be poisoned")
            .take()
        {
            let _ = join.join();
        }

        let _ = self
            .inner
            .stdin
            .lock()
            .expect("watchdog stdin lock should not be poisoned")
            .take();

        if let Some(mut child) = self
            .inner
            .child
            .lock()
            .expect("watchdog child lock should not be poisoned")
            .take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn start_auto_heartbeat(&self, check_ms: u64) {
        let handle = self.clone();
        let join = thread::spawn(move || {
            while !handle.stopped() {
                handle.heartbeat();
                thread::sleep(Duration::from_millis(check_ms));
            }
        });
        *self
            .inner
            .heartbeat
            .lock()
            .expect("watchdog heartbeat lock should not be poisoned") = Some(join);
    }
}

pub type DeferredWatchdogHandle = MainThreadWatchdogHandle;

impl WatchdogHandle for MainThreadWatchdogHandle {
    fn stop(&self) {
        MainThreadWatchdogHandle::stop(self);
    }
}

pub fn parse_watchdog_timeout_ms(raw: Option<&str>, fallback: u64) -> u64 {
    let Some(raw) = raw else {
        return fallback;
    };
    raw.parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

pub fn derive_check_interval_ms(timeout_ms: u64) -> u64 {
    (timeout_ms / 5).clamp(50, 2_000)
}

pub fn is_env_truthy(raw: Option<&str>) -> bool {
    raw.map(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
    .unwrap_or(false)
}

pub fn install_main_thread_watchdog() -> Option<DeferredWatchdogHandle> {
    // 环境变量显式关闭时完全不启动子进程，方便调试卡死现场。
    if is_env_truthy(first_env(&["RUSTCODEGRAPH_NO_WATCHDOG"]).as_deref()) {
        return None;
    }

    let timeout_ms = parse_watchdog_timeout_ms(
        first_env(&["RUSTCODEGRAPH_WATCHDOG_TIMEOUT_MS"]).as_deref(),
        DEFAULT_WATCHDOG_TIMEOUT_MS,
    );
    let check_ms = derive_check_interval_ms(timeout_ms);

    let mut child = match Command::new(node_bin())
        .arg("-e")
        .arg(CHILD_SOURCE)
        .arg(std::process::id().to_string())
        .arg(timeout_ms.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .current_dir(env::temp_dir())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            debug(&format!("spawn failed: {err}"));
            return None;
        }
    };

    let Some(stdin) = child.stdin.take() else {
        debug("child has no stdin pipe; not arming");
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };

    let handle = MainThreadWatchdogHandle::new(child, stdin);
    if !is_env_truthy(
        env::var("RUSTCODEGRAPH_WATCHDOG_MANUAL_HEARTBEAT")
            .ok()
            .as_deref(),
    ) {
        handle.start_auto_heartbeat(check_ms);
    }
    debug(&format!("armed: timeoutMs={timeout_ms} checkMs={check_ms}"));

    Some(handle)
}

fn first_env(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| env::var(name).ok())
}

fn node_bin() -> OsString {
    env::var_os("NODE").unwrap_or_else(|| OsString::from("node"))
}

fn debug(msg: &str) {
    if env::var_os("RUSTCODEGRAPH_MCP_DEBUG").is_some() {
        eprintln!("[RustCodeGraph watchdog] {msg}");
    }
}
