//! Last-resort fatal handler helpers.
//!
//! Rust does not have Node's `uncaughtException` / `unhandledRejection`
//! process events, so this module preserves the bounded formatting contract
//! and exposes injectable event-target wiring for a future CLI runtime.
//!
//! 这里抽象成 trait 而不是直接绑定运行时事件，是为了让 Rust 端测试能覆盖
//! “格式化失败也不 panic、最终一定退出”的兜底行为。

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

const UNSTRINGIFIABLE_VALUE: &str = "<unstringifiable value>";

/// 把外部运行时的错误对象压成 CLI 需要的最小形状。
pub trait FatalErrorLike {
    fn name(&self) -> &str;
    fn message(&self) -> &str;

    fn stack(&self) -> Option<&str> {
        None
    }
}

/// fatal handler 可能收到的值类型；Display 分支用于兼容非 Error 的 rejection reason。
#[derive(Clone, Copy)]
pub enum FatalValue<'a> {
    Error(&'a dyn FatalErrorLike),
    Display(&'a dyn fmt::Display),
    Null,
    Undefined,
}

/// 可测试的 fatal 文案中间态，避免每个调用点重复拼接 name/message。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FatalDescription {
    pub name: String,
    pub message: Option<String>,
}

impl FatalDescription {
    pub fn render(&self) -> String {
        match &self.message {
            Some(message) if !message.is_empty() => format!("{}: {}", self.name, message),
            _ => self.name.clone(),
        }
    }
}

pub fn describe_fatal(value: FatalValue<'_>) -> String {
    match value {
        FatalValue::Error(error) => FatalDescription {
            name: if error.name().is_empty() {
                "Error"
            } else {
                error.name()
            }
            .to_owned(),
            message: Some(error.message().to_owned()),
        }
        .render(),
        FatalValue::Display(value) => describe_display(value),
        FatalValue::Null => "null".to_owned(),
        FatalValue::Undefined => "undefined".to_owned(),
    }
}

struct StringSink(String);

impl fmt::Write for StringSink {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0.push_str(value);
        Ok(())
    }
}

pub fn describe_display(value: &dyn fmt::Display) -> String {
    let mut sink = StringSink(String::new());
    // Display 实现理论上可以返回 fmt::Error；fatal 路径必须降级成固定文案而不是二次崩溃。
    match fmt::write(&mut sink, format_args!("{}", value)) {
        Ok(()) => sink.0,
        Err(_) => UNSTRINGIFIABLE_VALUE.to_owned(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatalEvent {
    UncaughtException,
    UnhandledRejection,
}

pub type FatalEventHandler<'a> = Box<dyn for<'value> FnMut(FatalValue<'value>) + 'a>;

/// 可注入的事件源，给将来 CLI runtime 接入真实 fatal 事件留边界。
pub trait FatalEventTarget<'a> {
    fn on(&mut self, event: FatalEvent, handler: FatalEventHandler<'a>);
}

/// handler 的外部副作用全部走依赖注入，测试里可以替换写 stderr 和退出进程。
pub struct FatalHandlerDeps<'target, T, W, E>
where
    W: FnMut(&str),
    E: FnMut(i32),
    T: ?Sized,
{
    /// 事件注册目标，生产环境会映射到宿主运行时的 fatal 事件。
    pub target: &'target mut T,
    pub write: W,
    pub exit: E,
}

pub fn install_fatal_handlers<'handler, T, W, E>(deps: FatalHandlerDeps<'_, T, W, E>)
where
    T: FatalEventTarget<'handler> + ?Sized,
    W: FnMut(&str) + 'handler,
    E: FnMut(i32) + 'handler,
{
    let write = Rc::new(RefCell::new(deps.write));
    let exit = Rc::new(RefCell::new(deps.exit));

    // 两个 handler 共享同一组可变副作用；Rc<RefCell<_>> 让闭包保持 FnMut 接口且无需全局状态。
    let write_uncaught = Rc::clone(&write);
    let exit_uncaught = Rc::clone(&exit);
    deps.target.on(
        FatalEvent::UncaughtException,
        Box::new(move |error| {
            (write_uncaught.borrow_mut())(&format!(
                "[RustCodeGraph] Uncaught exception: {}\n",
                describe_fatal(error)
            ));
            (exit_uncaught.borrow_mut())(1);
        }),
    );

    let write_rejection = Rc::clone(&write);
    let exit_rejection = Rc::clone(&exit);
    deps.target.on(
        FatalEvent::UnhandledRejection,
        Box::new(move |reason| {
            (write_rejection.borrow_mut())(&format!(
                "[RustCodeGraph] Unhandled rejection: {}\n",
                describe_fatal(reason)
            ));
            (exit_rejection.borrow_mut())(1);
        }),
    );
}
