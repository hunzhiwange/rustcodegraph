//! Regression coverage for #850 (and the related #799): a fault that reaches
//! the process-wide handler must not be swallowed-and-kept-running, and
//! rendering it must never touch `error.stack`.
//!
//! This is the Rust port of `__tests__/fatal-handler.test.ts`.

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[allow(dead_code)]
#[path = "../src/bin/fatal_handler.rs"]
mod fatal_handler;

use fatal_handler::{
    FatalErrorLike, FatalEvent, FatalEventHandler, FatalEventTarget, FatalHandlerDeps, FatalValue,
    describe_fatal, install_fatal_handlers,
};

struct TestError {
    name: &'static str,
    message: &'static str,
}

impl TestError {
    fn new(name: &'static str, message: &'static str) -> Self {
        Self { name, message }
    }
}

impl FatalErrorLike for TestError {
    fn name(&self) -> &str {
        self.name
    }

    fn message(&self) -> &str {
        self.message
    }
}

struct StackTripwireError {
    name: &'static str,
    message: &'static str,
    stack_accessed: Arc<AtomicBool>,
}

impl StackTripwireError {
    fn new(message: &'static str, stack_accessed: Arc<AtomicBool>) -> Self {
        Self {
            name: "Error",
            message,
            stack_accessed,
        }
    }
}

impl FatalErrorLike for StackTripwireError {
    fn name(&self) -> &str {
        self.name
    }

    fn message(&self) -> &str {
        self.message
    }

    fn stack(&self) -> Option<&str> {
        self.stack_accessed.store(true, Ordering::SeqCst);
        panic!("stack formatting wedged");
    }
}

struct HostileToString;

impl fmt::Display for HostileToString {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Err(fmt::Error)
    }
}

#[derive(Default)]
struct TestEventTarget<'handler> {
    uncaught_exception_handlers: Vec<FatalEventHandler<'handler>>,
    unhandled_rejection_handlers: Vec<FatalEventHandler<'handler>>,
}

impl<'handler> TestEventTarget<'handler> {
    fn emit(&mut self, event: FatalEvent, value: FatalValue<'_>) {
        let handlers = match event {
            FatalEvent::UncaughtException => &mut self.uncaught_exception_handlers,
            FatalEvent::UnhandledRejection => &mut self.unhandled_rejection_handlers,
        };

        for handler in handlers {
            handler(value);
        }
    }
}

impl<'handler> FatalEventTarget<'handler> for TestEventTarget<'handler> {
    fn on(&mut self, event: FatalEvent, handler: FatalEventHandler<'handler>) {
        match event {
            FatalEvent::UncaughtException => self.uncaught_exception_handlers.push(handler),
            FatalEvent::UnhandledRejection => self.unhandled_rejection_handlers.push(handler),
        }
    }
}

fn values<T: Clone>(values: &Arc<Mutex<Vec<T>>>) -> Vec<T> {
    values
        .lock()
        .expect("test vector mutex should not be poisoned")
        .clone()
}

mod describe_fatal {
    use super::*;

    #[test]
    fn renders_name_message_for_an_error() {
        let error = TestError::new("TypeError", "boom");
        assert_eq!(describe_fatal(FatalValue::Error(&error)), "TypeError: boom");
    }

    #[test]
    fn falls_back_to_the_name_when_the_message_is_empty() {
        let error = TestError::new("Error", "");
        assert_eq!(describe_fatal(FatalValue::Error(&error)), "Error");
    }

    #[test]
    fn stringifies_non_error_values() {
        assert_eq!(
            describe_fatal(FatalValue::Display(&"a string reason")),
            "a string reason"
        );
        assert_eq!(describe_fatal(FatalValue::Display(&42)), "42");
        assert_eq!(describe_fatal(FatalValue::Null), "null");
        assert_eq!(describe_fatal(FatalValue::Undefined), "undefined");
    }

    #[test]
    fn never_reads_error_stack_the_850_hang_lives_in_the_lazy_stack_getter() {
        let stack_accessed = Arc::new(AtomicBool::new(false));
        let error = StackTripwireError::new("boom", Arc::clone(&stack_accessed));

        let rendered = describe_fatal(FatalValue::Error(&error));

        assert!(!stack_accessed.load(Ordering::SeqCst));
        assert_eq!(rendered, "Error: boom");
        assert!(!regex::Regex::new(r"\bat\b").unwrap().is_match(&rendered));
    }

    #[test]
    fn never_throws_on_a_value_with_a_hostile_to_string() {
        let hostile = HostileToString;
        let rendered = catch_unwind(AssertUnwindSafe(|| {
            describe_fatal(FatalValue::Display(&hostile))
        }))
        .expect("describe_fatal should not panic");
        assert_eq!(rendered, "<unstringifiable value>");
    }
}

mod install_fatal_handlers {
    use super::*;

    struct Harness {
        target: TestEventTarget<'static>,
        writes: Arc<Mutex<Vec<String>>>,
        exits: Arc<Mutex<Vec<i32>>>,
    }

    fn harness() -> Harness {
        let mut target = TestEventTarget::default();
        let writes = Arc::new(Mutex::new(Vec::<String>::new()));
        let exits = Arc::new(Mutex::new(Vec::<i32>::new()));

        install_fatal_handlers(FatalHandlerDeps {
            target: &mut target,
            write: {
                let writes = Arc::clone(&writes);
                move |line| {
                    writes
                        .lock()
                        .expect("writes mutex should not be poisoned")
                        .push(line.to_owned());
                }
            },
            exit: {
                let exits = Arc::clone(&exits);
                move |code| {
                    exits
                        .lock()
                        .expect("exits mutex should not be poisoned")
                        .push(code);
                }
            },
        });

        Harness {
            target,
            writes,
            exits,
        }
    }

    #[test]
    fn logs_a_bounded_line_and_exits_non_zero_on_an_uncaught_exception() {
        let mut harness = harness();
        let error = TestError::new("RangeError", "kaboom");
        harness
            .target
            .emit(FatalEvent::UncaughtException, FatalValue::Error(&error));

        assert_eq!(
            values(&harness.writes),
            vec!["[RustCodeGraph] Uncaught exception: RangeError: kaboom\n".to_owned()]
        );
        assert_eq!(values(&harness.exits), vec![1]);
    }

    #[test]
    fn logs_a_bounded_line_and_exits_non_zero_on_an_unhandled_rejection() {
        let mut harness = harness();
        harness.target.emit(
            FatalEvent::UnhandledRejection,
            FatalValue::Display(&"promise went sideways"),
        );

        assert_eq!(
            values(&harness.writes),
            vec!["[RustCodeGraph] Unhandled rejection: promise went sideways\n".to_owned()]
        );
        assert_eq!(values(&harness.exits), vec![1]);
    }

    #[test]
    fn still_exits_without_touching_the_stack_when_stack_formatting_would_wedge() {
        let mut harness = harness();
        let stack_accessed = Arc::new(AtomicBool::new(false));
        let error = StackTripwireError::new("wedged", Arc::clone(&stack_accessed));

        let result = catch_unwind(AssertUnwindSafe(|| {
            harness
                .target
                .emit(FatalEvent::UncaughtException, FatalValue::Error(&error));
        }));

        assert!(result.is_ok(), "handler should not panic");
        assert!(!stack_accessed.load(Ordering::SeqCst));
        assert_eq!(
            values(&harness.writes),
            vec!["[RustCodeGraph] Uncaught exception: Error: wedged\n".to_owned()]
        );
        assert_eq!(values(&harness.exits), vec![1]);
    }
}
