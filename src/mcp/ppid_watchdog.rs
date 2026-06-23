//! Shared PPID watchdog decision logic translated from `ppid-watchdog.ts`.
//!
//! PPID watchdog 只做判定，不执行退出；调用方决定轮询间隔和日志。

/// State sampled by the PPID watchdog.
#[derive(Debug, Clone)]
pub struct SupervisionState<'a, F>
where
    F: Fn(u32) -> bool,
{
    pub original_ppid: u32,
    pub current_ppid: u32,
    pub host_ppid: Option<u32>,
    pub is_alive: F,
    pub platform: Option<&'a str>,
}

/// Return a human-readable reason when supervision has been lost.
pub fn supervision_lost_reason<F>(state: SupervisionState<'_, F>) -> Option<String>
where
    F: Fn(u32) -> bool,
{
    let platform = state.platform.unwrap_or(std::env::consts::OS);

    if state.current_ppid != state.original_ppid {
        return Some(format!(
            "ppid {} -> {}",
            state.original_ppid, state.current_ppid
        ));
    }

    if platform == "windows" && state.original_ppid > 1 && !(state.is_alive)(state.original_ppid) {
        // Windows 的 PPID 可能不变但父进程已经退出，需要显式查存活状态。
        return Some(format!("parent pid {} exited", state.original_ppid));
    }

    if let Some(host_pid) = state.host_ppid
        && !(state.is_alive)(host_pid)
    {
        return Some(format!("host pid {host_pid} exited"));
    }

    None
}
