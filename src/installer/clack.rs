//! installer 使用的 `@clack/prompts` 最小 Rust facade。
//!
//! 这里定义的是交互边界而不是具体终端实现，方便安装器测试用 fake prompt 驱动同一套流程。

#[derive(Debug, Clone)]
pub struct SelectOption<T> {
    pub value: T,
    pub label: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConfirmOptions {
    pub message: String,
    pub active: Option<String>,
    pub inactive: Option<String>,
    pub initial_value: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct SelectOptions<T> {
    pub message: String,
    pub options: Vec<SelectOption<T>>,
    pub initial_value: Option<T>,
}

#[derive(Debug, Clone)]
pub struct MultiSelectOptions<T> {
    pub message: String,
    pub options: Vec<SelectOption<T>>,
    pub initial_values: Vec<T>,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

pub type PromptResult<T> = Result<T, Cancelled>;

pub trait Spinner {
    fn start(&mut self, message: Option<&str>);
    fn stop(&mut self, message: Option<&str>);
    fn message(&mut self, message: Option<&str>);
}

pub trait Clack {
    fn intro(&self, title: Option<&str>);
    fn outro(&self, message: Option<&str>);
    fn cancel(&self, message: Option<&str>);
    fn confirm(&self, opts: ConfirmOptions) -> PromptResult<bool>;
    fn select<T: Clone>(&self, opts: SelectOptions<T>) -> PromptResult<T>;
    fn multiselect<T: Clone>(&self, opts: MultiSelectOptions<T>) -> PromptResult<Vec<T>>;
    fn note(&self, message: &str, title: Option<&str>);
    fn log(&self) -> &dyn ClackLog;
}

pub trait ClackLog {
    fn message(&self, message: &str);
    fn info(&self, message: &str);
    fn success(&self, message: &str);
    fn step(&self, message: &str);
    fn warn(&self, message: &str);
    fn error(&self, message: &str);
}
