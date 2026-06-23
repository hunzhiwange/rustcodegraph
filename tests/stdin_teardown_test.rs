//! #799 - a socket-backed stdin that fails must shut the server down, not
//! orphan/busy-spin. `treat_stdin_failure_as_shutdown` is the shared guard.
//!
//! This is the Rust port of `__tests__/stdin-teardown.test.ts`.

mod treat_stdin_failure_as_shutdown_799 {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use rustcodegraph::mcp::stdin_teardown::{
        StdinTeardownStream, StdinTerminalEvent, treat_stdin_stream_failure_as_shutdown,
    };

    type Handler = Arc<dyn Fn() + Send + Sync>;

    #[derive(Clone, Default)]
    struct PassThrough {
        inner: Arc<Mutex<PassThroughInner>>,
    }

    #[derive(Default)]
    struct PassThroughInner {
        end_handlers: Vec<Handler>,
        close_handlers: Vec<Handler>,
        error_handlers: Vec<Handler>,
        destroyed: bool,
    }

    impl PassThrough {
        fn emit(&self, event: StdinTerminalEvent) {
            let handlers = {
                let inner = self.inner.lock().expect("pass-through lock poisoned");
                match event {
                    StdinTerminalEvent::End => inner.end_handlers.clone(),
                    StdinTerminalEvent::Close => inner.close_handlers.clone(),
                    StdinTerminalEvent::Error => inner.error_handlers.clone(),
                }
            };

            for handler in handlers {
                handler();
            }
        }

        fn destroyed(&self) -> bool {
            self.inner
                .lock()
                .expect("pass-through lock poisoned")
                .destroyed
        }
    }

    impl StdinTeardownStream for PassThrough {
        fn on_terminal_event(&self, event: StdinTerminalEvent, handler: Handler) {
            let mut inner = self.inner.lock().expect("pass-through lock poisoned");
            match event {
                StdinTerminalEvent::End => inner.end_handlers.push(handler),
                StdinTerminalEvent::Close => inner.close_handlers.push(handler),
                StdinTerminalEvent::Error => inner.error_handlers.push(handler),
            }
        }

        fn destroy(&self) {
            let close_handlers = {
                let mut inner = self.inner.lock().expect("pass-through lock poisoned");
                if inner.destroyed {
                    return;
                }
                inner.destroyed = true;
                inner.close_handlers.clone()
            };

            for handler in close_handlers {
                handler();
            }
        }
    }

    #[test]
    fn treats_a_stdin_error_econnreset_hangup_as_a_shutdown_signal() {
        let stream = PassThrough::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_guard = Arc::clone(&calls);
        let _guard = treat_stdin_stream_failure_as_shutdown(
            move || {
                calls_for_guard.fetch_add(1, Ordering::SeqCst);
            },
            stream.clone(),
        );

        // No extra 'error' listener would throw here - the guard registers one.
        stream.emit(StdinTerminalEvent::Error);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn also_fires_on_end_and_on_close() {
        for event in [StdinTerminalEvent::End, StdinTerminalEvent::Close] {
            let stream = PassThrough::default();
            let calls = Arc::new(AtomicUsize::new(0));
            let calls_for_guard = Arc::clone(&calls);
            let _guard = treat_stdin_stream_failure_as_shutdown(
                move || {
                    calls_for_guard.fetch_add(1, Ordering::SeqCst);
                },
                stream.clone(),
            );

            stream.emit(event);
            assert_eq!(calls.load(Ordering::SeqCst), 1, "event {event:?}");
        }
    }

    #[test]
    fn destroys_the_stream_so_a_hung_fd_leaves_epoll() {
        let stream = PassThrough::default();
        let _guard = treat_stdin_stream_failure_as_shutdown(|| {}, stream.clone());

        stream.emit(StdinTerminalEvent::Error);
        assert!(stream.destroyed());
    }

    #[test]
    fn fires_on_terminal_at_most_once_even_across_error_to_close() {
        let stream = PassThrough::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_guard = Arc::clone(&calls);
        let _guard = treat_stdin_stream_failure_as_shutdown(
            move || {
                calls_for_guard.fetch_add(1, Ordering::SeqCst);
            },
            stream.clone(),
        );

        stream.emit(StdinTerminalEvent::Error); // fire() also destroys -> emits 'close'
        stream.emit(StdinTerminalEvent::Close); // must not double-fire
        stream.emit(StdinTerminalEvent::End);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
