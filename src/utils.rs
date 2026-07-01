//! CodeGraph utilities translated from `utils.ts`.
//!
//! 这里放跨模块共享但不属于某个业务层的工具：路径安全、文件锁、批处理、
//! 节流/防抖、粗略内存监控等。很多函数被 MCP、context 和索引流程共同使用，
//! 所以边界条件比实现本身更重要。

use std::collections::HashSet;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::io::{self, Read, Write};
use std::marker::PhantomData;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex as StdMutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::errors::{CodeGraphError, FileError};

const SENSITIVE_PATHS: &[&str] = &[
    "/",
    "/etc",
    "/usr",
    "/bin",
    "/sbin",
    "/var",
    "/tmp",
    "/dev",
    "/proc",
    "/sys",
    "/root",
    "/boot",
    "/lib",
    "/lib64",
    "/opt",
    "c:\\",
    "c:\\windows",
    "c:\\windows\\system32",
];

/// YAML/properties 的叶子键会抽成 constant，但不应继续当普通符号向下展开。
pub static CONFIG_LEAF_LANGUAGES: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["yaml", "properties"]));

pub fn is_config_leaf_node(kind: &str, language: Option<&str>) -> bool {
    kind == "constant"
        && language
            .map(|language| CONFIG_LEAF_LANGUAGES.contains(language))
            .unwrap_or(false)
}

pub fn validate_path_within_root(
    project_root: impl AsRef<Path>,
    file_path: impl AsRef<Path>,
) -> Option<PathBuf> {
    let normalized_root = absolutize(project_root.as_ref());
    let resolved = resolve_from(&normalized_root, file_path.as_ref());

    // 先做词法检查挡住明显的 ../ 和绝对路径注入，随后再 canonicalize 防 symlink 逃逸。
    if !is_within_dir(&resolved, &normalized_root) {
        return None;
    }

    let real_root = match fs::canonicalize(&normalized_root) {
        Ok(path) => path,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Some(resolved),
        Err(_) => return None,
    };
    let real_resolved = match fs::canonicalize(&resolved) {
        Ok(path) => path,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Some(resolved),
        Err(_) => return None,
    };

    if is_within_dir(&real_resolved, &real_root) {
        Some(real_resolved)
    } else {
        None
    }
}

pub fn validate_project_path(dir_path: impl AsRef<Path>) -> Option<String> {
    let resolved = absolutize(dir_path.as_ref());
    let resolved_text = resolved.to_string_lossy().to_string();
    let resolved_lower = resolved_text.to_lowercase();

    if SENSITIVE_PATHS.contains(&resolved_text.as_str())
        || SENSITIVE_PATHS.contains(&resolved_lower.as_str())
    {
        return Some(format!(
            "Refusing to operate on sensitive system directory: {resolved_text}"
        ));
    }

    if let Some(home_dir) = home_dir() {
        // 用户主目录下的密钥/云凭据目录即使存在也不应作为项目根参与索引或 MCP 操作。
        for dir in [".ssh", ".gnupg", ".aws", ".config"] {
            let sensitive_path = home_dir.join(dir);
            if is_within_dir(&resolved, &sensitive_path) {
                return Some(format!(
                    "Refusing to operate on sensitive directory: {resolved_text}"
                ));
            }
        }
    }

    match fs::metadata(&resolved) {
        Ok(metadata) if metadata.is_dir() => None,
        Ok(_) => Some(format!("Path is not a directory: {resolved_text}")),
        Err(_) => Some(format!(
            "Path does not exist or is not accessible: {resolved_text}"
        )),
    }
}

pub fn safe_json_parse<T>(value: &str, fallback: T) -> T
where
    T: DeserializeOwned,
{
    serde_json::from_str(value).unwrap_or(fallback)
}

pub fn clamp<T>(value: T, min: T, max: T) -> T
where
    T: PartialOrd + Copy,
{
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

pub fn normalize_path(file_path: &str) -> String {
    file_path.replace('\\', "/")
}

pub struct FileLock {
    lock_path: PathBuf,
    held: bool,
}

impl FileLock {
    const STALE_TIMEOUT_MS: u64 = 2 * 60 * 1000;

    pub fn new(lock_path: impl Into<PathBuf>) -> Self {
        Self {
            lock_path: lock_path.into(),
            held: false,
        }
    }

    pub fn acquire(&mut self) -> Result<(), CodeGraphError> {
        if self.lock_path.exists() {
            match self.read_lock_pid_and_age() {
                Ok((Some(pid), age)) if age < Duration::from_millis(Self::STALE_TIMEOUT_MS) => {
                    // 新鲜锁只在持有进程仍存活时阻塞；已退出进程留下的锁会自动清理。
                    if self.is_process_alive(pid) {
                        return Err(CodeGraphError::new(
                            format!(
                                "RustCodeGraph database is locked by another process (PID {pid}). \
                                 If this is stale, run 'rustcodegraph unlock' or delete {}",
                                self.lock_path.display()
                            ),
                            "LOCK_ERROR",
                            None,
                        ));
                    }
                    let _ = fs::remove_file(&self.lock_path);
                }
                Ok(_) => {
                    let _ = fs::remove_file(&self.lock_path);
                }
                Err(_) => {
                    let _ = fs::remove_file(&self.lock_path);
                }
            }
        }

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.lock_path)
        {
            Ok(mut file) => {
                write!(file, "{}", std::process::id())
                    .map_err(|err| file_error("Failed to write lock file", &self.lock_path, err))?;
                self.held = true;
                Ok(())
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Err(CodeGraphError::new(
                format!(
                    "RustCodeGraph database is locked by another process. If this is stale, run \
                     'rustcodegraph unlock' or delete {}",
                    self.lock_path.display()
                ),
                "LOCK_ERROR",
                None,
            )),
            Err(err) => Err(file_error(
                "Failed to create lock file",
                &self.lock_path,
                err,
            )),
        }
    }

    pub fn release(&mut self) {
        if !self.held {
            return;
        }

        // 只删除自己写下的锁，避免误删另一个刚刚获取到的进程锁。
        if let Ok(content) = fs::read_to_string(&self.lock_path)
            && content.trim().parse::<u32>().ok() == Some(std::process::id())
        {
            let _ = fs::remove_file(&self.lock_path);
        }

        self.held = false;
    }

    pub fn with_lock<T, F>(&mut self, f: F) -> Result<T, CodeGraphError>
    where
        F: FnOnce() -> T,
    {
        self.acquire()?;
        // 即使闭包 panic 也先释放锁，再把 panic 原样抛回调用方。
        let result = panic::catch_unwind(AssertUnwindSafe(f));
        self.release();
        match result {
            Ok(value) => Ok(value),
            Err(payload) => panic::resume_unwind(payload),
        }
    }

    pub async fn with_lock_async<T, F, Fut>(&mut self, f: F) -> Result<T, CodeGraphError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        self.acquire()?;
        let result = f().await;
        self.release();
        Ok(result)
    }

    fn read_lock_pid_and_age(&self) -> Result<(Option<u32>, Duration), CodeGraphError> {
        let content = fs::read_to_string(&self.lock_path)
            .map_err(|err| file_error("Failed to read lock file", &self.lock_path, err))?;
        let pid = content.trim().parse::<u32>().ok();
        let modified = fs::metadata(&self.lock_path)
            .map_err(|err| file_error("Failed to stat lock file", &self.lock_path, err))?
            .modified()
            .map_err(|err| file_error("Failed to inspect lock file", &self.lock_path, err))?;
        let age = modified.elapsed().unwrap_or(Duration::ZERO);
        Ok((pid, age))
    }

    fn is_process_alive(&self, pid: u32) -> bool {
        is_process_alive(pid)
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        self.release();
    }
}

pub async fn process_in_batches<T, R, F, Fut, C>(
    items: &[T],
    batch_size: usize,
    mut processor: F,
    mut on_batch_complete: Option<C>,
) -> Vec<R>
where
    F: FnMut(&T, usize) -> Fut,
    Fut: Future<Output = R>,
    C: FnMut(usize, usize),
{
    let mut results = Vec::new();
    let effective_batch_size = batch_size.max(1);

    for (batch_index, batch) in items.chunks(effective_batch_size).enumerate() {
        let offset = batch_index * effective_batch_size;
        // 这里按批顺序 await，不做并发；batch 的意义是进度回调和内存节奏，而不是并行。
        for (idx, item) in batch.iter().enumerate() {
            results.push(processor(item, offset + idx).await);
        }

        if let Some(callback) = on_batch_complete.as_mut() {
            callback((offset + batch.len()).min(items.len()), items.len());
        }
    }

    results
}

#[derive(Debug, Default)]
pub struct Mutex {
    inner: StdMutex<()>,
}

impl Mutex {
    pub fn new() -> Self {
        Self {
            inner: StdMutex::new(()),
        }
    }

    pub fn acquire(&self) -> MutexGuard<'_, ()> {
        self.inner.lock().expect("mutex lock poisoned")
    }

    pub fn with_lock<T, F>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        let _guard = self.acquire();
        f()
    }

    pub fn is_locked(&self) -> bool {
        // try_lock 成功会立刻丢弃 guard；这里只作为轻量状态探测给测试/兼容层使用。
        self.inner.try_lock().is_err()
    }
}

pub const DEFAULT_CHUNK_SIZE: usize = 64 * 1024;

pub struct FileChunks {
    file: File,
    path: PathBuf,
    chunk_size: usize,
}

impl Iterator for FileChunks {
    type Item = Result<String, CodeGraphError>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buffer = vec![0_u8; self.chunk_size];
        match self.file.read(&mut buffer) {
            Ok(0) => None,
            Ok(bytes_read) => {
                buffer.truncate(bytes_read);
                // 大文件上下文展示宁可损失非法 UTF-8 字节，也不要让一次解码失败中断读取。
                Some(Ok(String::from_utf8_lossy(&buffer).to_string()))
            }
            Err(err) => Some(Err(file_error(
                "Failed to read file chunk",
                &self.path,
                err,
            ))),
        }
    }
}

pub fn read_file_in_chunks(
    file_path: impl AsRef<Path>,
    chunk_size: Option<usize>,
) -> Result<FileChunks, CodeGraphError> {
    let path = file_path.as_ref();
    let file = File::open(path).map_err(|err| file_error("Failed to open file", path, err))?;
    Ok(FileChunks {
        file,
        path: path.to_path_buf(),
        chunk_size: chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE).max(1),
    })
}

pub struct Debounced<F, A>
where
    F: Fn(A) + Send + Sync + 'static,
    A: Send + 'static,
{
    func: Arc<F>,
    delay: Duration,
    generation: Arc<AtomicU64>,
    _args: PhantomData<A>,
}

impl<F, A> Clone for Debounced<F, A>
where
    F: Fn(A) + Send + Sync + 'static,
    A: Send + 'static,
{
    fn clone(&self) -> Self {
        Self {
            func: Arc::clone(&self.func),
            delay: self.delay,
            generation: Arc::clone(&self.generation),
            _args: PhantomData,
        }
    }
}

impl<F, A> Debounced<F, A>
where
    F: Fn(A) + Send + Sync + 'static,
    A: Send + 'static,
{
    pub fn call(&self, args: A) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let current_generation = Arc::clone(&self.generation);
        let func = Arc::clone(&self.func);
        let delay = self.delay;
        thread::spawn(move || {
            thread::sleep(delay);
            // 只有最后一次调用对应的 generation 能落地，旧线程自然过期。
            if current_generation.load(Ordering::SeqCst) == generation {
                func(args);
            }
        });
    }
}

pub fn debounce<F, A>(func: F, delay_ms: u64) -> Debounced<F, A>
where
    F: Fn(A) + Send + Sync + 'static,
    A: Send + 'static,
{
    Debounced {
        func: Arc::new(func),
        delay: Duration::from_millis(delay_ms),
        generation: Arc::new(AtomicU64::new(0)),
        _args: PhantomData,
    }
}

pub struct Throttled<F, A>
where
    F: Fn(A) + Send + Sync + 'static,
    A: Send + 'static,
{
    func: Arc<F>,
    limit: Duration,
    last_call: Arc<StdMutex<Option<Instant>>>,
    scheduled: Arc<AtomicBool>,
    _args: PhantomData<A>,
}

impl<F, A> Clone for Throttled<F, A>
where
    F: Fn(A) + Send + Sync + 'static,
    A: Send + 'static,
{
    fn clone(&self) -> Self {
        Self {
            func: Arc::clone(&self.func),
            limit: self.limit,
            last_call: Arc::clone(&self.last_call),
            scheduled: Arc::clone(&self.scheduled),
            _args: PhantomData,
        }
    }
}

impl<F, A> Throttled<F, A>
where
    F: Fn(A) + Send + Sync + 'static,
    A: Send + 'static,
{
    pub fn call(&self, args: A) {
        let now = Instant::now();
        let mut last_call = self.last_call.lock().expect("throttle lock poisoned");
        let remaining = last_call
            .map(|last| {
                self.limit
                    .saturating_sub(now.saturating_duration_since(last))
            })
            .unwrap_or(Duration::ZERO);

        if remaining.is_zero() {
            *last_call = Some(now);
            drop(last_call);
            (self.func)(args);
            return;
        }

        if !self.scheduled.swap(true, Ordering::SeqCst) {
            let func = Arc::clone(&self.func);
            let last_call = Arc::clone(&self.last_call);
            let scheduled = Arc::clone(&self.scheduled);
            thread::spawn(move || {
                thread::sleep(remaining);
                // 已有延迟调用时，后续参数会被丢弃；调用方不应依赖 throttle 保留最新值。
                *last_call.lock().expect("throttle lock poisoned") = Some(Instant::now());
                scheduled.store(false, Ordering::SeqCst);
                func(args);
            });
        }
    }
}

pub fn throttle<F, A>(func: F, limit_ms: u64) -> Throttled<F, A>
where
    F: Fn(A) + Send + Sync + 'static,
    A: Send + 'static,
{
    Throttled {
        func: Arc::new(func),
        limit: Duration::from_millis(limit_ms),
        last_call: Arc::new(StdMutex::new(None)),
        scheduled: Arc::new(AtomicBool::new(false)),
        _args: PhantomData,
    }
}

pub fn estimate_size(value: &Value) -> usize {
    // 这是 JSON 结构的粗略内存估算，用于预算判断，不等价于 serde 序列化长度。
    fn size_of(value: &Value) -> usize {
        match value {
            Value::Null => 0,
            Value::Bool(_) => 4,
            Value::Number(_) => 8,
            Value::String(value) => 2 * value.len(),
            Value::Array(values) => values.iter().map(size_of).sum(),
            Value::Object(map) => map
                .iter()
                .map(|(key, value)| 2 * key.len() + size_of(value))
                .sum(),
        }
    }

    size_of(value)
}

pub struct MemoryMonitor {
    threshold: u64,
    on_threshold_exceeded: Option<Arc<dyn Fn(u64) + Send + Sync>>,
    peak_usage: Arc<AtomicU64>,
    stop_signal: Arc<AtomicBool>,
    check_thread: Option<JoinHandle<()>>,
}

impl MemoryMonitor {
    pub fn new(
        threshold_mb: Option<u64>,
        on_threshold_exceeded: Option<Arc<dyn Fn(u64) + Send + Sync>>,
    ) -> Self {
        Self {
            threshold: threshold_mb.unwrap_or(500) * 1024 * 1024,
            on_threshold_exceeded,
            peak_usage: Arc::new(AtomicU64::new(0)),
            stop_signal: Arc::new(AtomicBool::new(false)),
            check_thread: None,
        }
    }

    pub fn start(&mut self, interval_ms: Option<u64>) {
        self.stop();
        self.peak_usage.store(0, Ordering::SeqCst);
        self.stop_signal.store(false, Ordering::SeqCst);

        let interval = Duration::from_millis(interval_ms.unwrap_or(1000).max(1));
        let threshold = self.threshold;
        let peak_usage = Arc::clone(&self.peak_usage);
        let stop_signal = Arc::clone(&self.stop_signal);
        let on_threshold_exceeded = self.on_threshold_exceeded.clone();

        self.check_thread = Some(thread::spawn(move || {
            while !stop_signal.load(Ordering::SeqCst) {
                let usage = current_memory_usage_bytes();
                // 监控线程只记录峰值和触发回调，不主动终止索引流程。
                peak_usage.fetch_max(usage, Ordering::SeqCst);
                if usage > threshold
                    && let Some(callback) = &on_threshold_exceeded
                {
                    callback(usage);
                }
                thread::sleep(interval);
            }
        }));
    }

    pub fn stop(&mut self) {
        self.stop_signal.store(true, Ordering::SeqCst);
        if let Some(handle) = self.check_thread.take() {
            let _ = handle.join();
        }
    }

    pub fn get_peak_usage(&self) -> u64 {
        self.peak_usage.load(Ordering::SeqCst)
    }

    pub fn get_current_usage(&self) -> u64 {
        current_memory_usage_bytes()
    }
}

impl Drop for MemoryMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn file_error(message: &str, path: &Path, err: io::Error) -> CodeGraphError {
    FileError::new(message, path.display().to_string(), Some(err.to_string())).into()
}

fn is_within_dir(child: &Path, parent: &Path) -> bool {
    if cfg!(windows) {
        // Windows 路径比较需要大小写不敏感，并补分隔符避免 C:\foo 匹配 C:\foobar。
        let child = child.to_string_lossy().to_lowercase();
        let parent = parent.to_string_lossy().to_lowercase();
        child == parent || child.starts_with(&append_separator(&parent))
    } else {
        child == parent || child.starts_with(parent)
    }
}

fn resolve_from(root: &Path, file_path: &Path) -> PathBuf {
    if file_path.is_absolute() {
        normalize_lexically(file_path)
    } else {
        normalize_lexically(&root.join(file_path))
    }
}

fn absolutize(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    normalize_lexically(&absolute)
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // 词法 pop 不解析 symlink；真正的 symlink 防护在 canonicalize 阶段完成。
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

fn append_separator(path: &str) -> String {
    if path.ends_with(std::path::MAIN_SEPARATOR) {
        path.to_string()
    } else {
        format!("{path}{}", std::path::MAIN_SEPARATOR)
    }
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .or_else(|| {
            let drive = env::var_os("HOMEDRIVE")?;
            let path = env::var_os("HOMEPATH")?;
            let mut joined = PathBuf::from(drive);
            joined.push(path);
            Some(joined.into_os_string())
        })
        .map(PathBuf::from)
}

#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    // signal 0 只做存在性/权限检查，不向目标进程发送实际信号。
    unsafe { kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    use std::ffi::c_void;

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> *mut c_void;
        fn GetExitCodeProcess(hProcess: *mut c_void, lpExitCode: *mut u32) -> i32;
        fn CloseHandle(hObject: *mut c_void) -> i32;
    }

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }

    let mut exit_code = 0;
    let ok = unsafe { GetExitCodeProcess(handle, &mut exit_code) != 0 };
    unsafe {
        let _ = CloseHandle(handle);
    }
    ok && exit_code == STILL_ACTIVE
}

#[cfg(all(not(unix), not(windows)))]
fn is_process_alive(_pid: u32) -> bool {
    false
}

#[cfg(target_os = "linux")]
fn current_memory_usage_bytes() -> u64 {
    // statm 第二列是 resident pages；这里假设常见 4 KiB page，足够做阈值提示。
    let Ok(statm) = fs::read_to_string("/proc/self/statm") else {
        return 0;
    };
    let Some(pages) = statm
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return 0;
    };
    pages * 4096
}

#[cfg(target_os = "macos")]
fn current_memory_usage_bytes() -> u64 {
    // macOS 没有 /proc，使用 ps rss(KiB) 作为轻量近似。
    let Ok(output) = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
    else {
        return 0;
    };
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .map(|kb| kb * 1024)
        .unwrap_or(0)
}

#[cfg(windows)]
fn current_memory_usage_bytes() -> u64 {
    let Ok(output) = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("(Get-Process -Id {}).WorkingSet64", std::process::id()),
        ])
        .output()
    else {
        return 0;
    };
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or(0)
}

#[cfg(all(not(target_os = "linux"), not(target_os = "macos"), not(windows)))]
fn current_memory_usage_bytes() -> u64 {
    0
}

/// TEMP debug: print current RSS at a labeled phase when RUSTCODEGRAPH_DEBUG_RSS is set.
pub fn debug_rss(label: &str) {
    if std::env::var("RUSTCODEGRAPH_DEBUG_RSS").is_ok() {
        use std::io::Write;
        let mb = current_memory_usage_bytes() / (1024 * 1024);
        let mut err = std::io::stderr();
        let _ = writeln!(err, "[RSS] {mb:>6} MB  {label}");
        let _ = err.flush();
    }
}

/// Compatibility injection point retained for callers from the removed watch
/// memory guard. Built-in watch sync no longer consults this reader.
type WatchMemoryReader = Box<dyn Fn() -> u64 + Send + Sync>;

static WATCH_MEMORY_READER: LazyLock<StdMutex<Option<WatchMemoryReader>>> =
    LazyLock::new(|| StdMutex::new(None));

/// Returns the current process memory reading, optionally overridden by
/// [`set_watch_memory_reader_for_tests`].
///
/// This is retained for compatibility only; built-in watch sync no longer uses
/// process memory readings to decide whether to run.
#[deprecated(note = "built-in watch sync no longer skips based on process memory")]
pub fn current_watch_memory_usage_bytes() -> u64 {
    if let Ok(reader) = WATCH_MEMORY_READER.lock()
        && let Some(reader) = reader.as_ref()
    {
        return reader();
    }
    current_memory_usage_bytes()
}

/// Overrides [`current_watch_memory_usage_bytes`] for compatibility tests.
///
/// Built-in watch sync no longer uses this hook.
#[deprecated(note = "built-in watch sync no longer skips based on process memory")]
pub fn set_watch_memory_reader_for_tests(reader: Option<WatchMemoryReader>) {
    if let Ok(mut slot) = WATCH_MEMORY_READER.lock() {
        *slot = reader;
    }
}
