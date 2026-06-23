//! Stdin teardown guard translated from `stdin-teardown.ts`.
//!
//! The TypeScript version wires Node stream events. The Rust counterpart keeps
//! the same single-shot terminal callback shape and exposes an injectable
//! stream hook for tests without registering process-global stdin handlers.
//!
//! MCP stdio 的宿主断开通常表现为 stdin end/close/error；这个 guard 把三种
//! 事件折叠成一次 shutdown 回调，避免重复清理。

use std::fmt;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdinTerminalEvent {
    End,
    Close,
    Error,
}

pub trait StdinTeardownStream: Clone + Send + Sync + 'static {
    fn on_terminal_event(&self, event: StdinTerminalEvent, handler: Arc<dyn Fn() + Send + Sync>);
    fn destroy(&self);
}

#[derive(Clone)]
pub struct StdinTeardownGuard {
    fired: Arc<AtomicBool>,
    on_terminal: Arc<dyn Fn() + Send + Sync>,
    destroy_stream: Arc<dyn Fn() + Send + Sync>,
}

impl fmt::Debug for StdinTeardownGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StdinTeardownGuard")
            .field("fired", &self.fired())
            .finish_non_exhaustive()
    }
}

impl StdinTeardownGuard {
    fn new<F, D>(on_terminal: F, destroy_stream: D) -> Self
    where
        F: Fn() + Send + Sync + 'static,
        D: Fn() + Send + Sync + 'static,
    {
        Self {
            fired: Arc::new(AtomicBool::new(false)),
            on_terminal: Arc::new(on_terminal),
            destroy_stream: Arc::new(destroy_stream),
        }
    }

    pub fn fire(&self) {
        // 多个终止事件可能连续到达；AtomicBool 保证 destroy 和回调只执行一次。
        if self.fired.swap(true, Ordering::SeqCst) {
            return;
        }
        (self.destroy_stream)();
        (self.on_terminal)();
    }

    pub fn fired(&self) -> bool {
        self.fired.load(Ordering::SeqCst)
    }
}

/// Create a single-shot terminal guard. Runtime stream registration is deferred.
pub fn treat_stdin_failure_as_shutdown<F>(on_terminal: F) -> StdinTeardownGuard
where
    F: Fn() + Send + Sync + 'static,
{
    StdinTeardownGuard::new(on_terminal, || {})
}

pub fn treat_stdin_stream_failure_as_shutdown<F, S>(on_terminal: F, stream: S) -> StdinTeardownGuard
where
    F: Fn() + Send + Sync + 'static,
    S: StdinTeardownStream,
{
    let stream_for_destroy = stream.clone();
    let guard = StdinTeardownGuard::new(on_terminal, move || {
        stream_for_destroy.destroy();
    });

    let fire = {
        let guard = guard.clone();
        Arc::new(move || guard.fire()) as Arc<dyn Fn() + Send + Sync>
    };
    stream.on_terminal_event(StdinTerminalEvent::End, fire.clone());
    stream.on_terminal_event(StdinTerminalEvent::Close, fire.clone());
    stream.on_terminal_event(StdinTerminalEvent::Error, fire);

    guard
}
