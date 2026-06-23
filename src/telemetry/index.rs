//! Anonymous usage telemetry client.
//!
//! This mirrors the TypeScript telemetry contract: recording is an in-memory
//! increment, stdout is never touched, "off" records and sends nothing, send
//! failures are silent, and usage counts are locally rolled up by completed UTC
//! day before sending.
//!
//! 这里承载的是“尽量不打扰用户”的遥测边界：调用方只负责记录意图，
//! 本模块负责同意状态、聚合、落盘、失败重试和首跑提示，且绝不把源码路径、
//! 符号名或 stdout 输出混入遥测流程。

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const TELEMETRY_ENDPOINT: &str = "https://telemetry.getcodegraph.com/v1/events";
pub const TELEMETRY_DOCS: &str =
    "https://github.com/hunzhiwange/rustcodegraph/blob/main/TELEMETRY.md";

const SCHEMA_VERSION: u32 = 1;
const MAX_BUFFER_BYTES: usize = 256 * 1024;
const MAX_EVENTS_PER_REQUEST: usize = 100;
const DEFAULT_FLUSH_TIMEOUT_MS: u64 = 1_500;
const STALE_CLAIM_MS: i128 = 60 * 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageKind {
    McpTool,
    CliCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleEvent {
    Install,
    Index,
    Uninstall,
}

/// 把真实文件规模压成粗粒度桶，避免上报精确项目大小。
pub fn bucket_file_count(n: u64) -> &'static str {
    if n < 100 {
        "<100"
    } else if n < 1_000 {
        "100-1k"
    } else if n < 10_000 {
        "1k-10k"
    } else {
        "10k+"
    }
}

/// 时长同样只保留产品诊断需要的量级，而不是精确性能轨迹。
pub fn bucket_duration(ms: u64) -> &'static str {
    if ms < 10_000 {
        "<10s"
    } else if ms < 60_000 {
        "10-60s"
    } else if ms < 300_000 {
        "1-5m"
    } else {
        "5m+"
    }
}

/// SQLite 后端名称可能来自不同层，遥测只关心 native/wasm 两类。
pub fn backend_kind(backend: &str) -> &'static str {
    if backend.to_lowercase().contains("wasm") {
        "wasm"
    } else {
        "native"
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConsentSource {
    Installer,
    DefaultNotice,
    Cli,
}

/// 持久化的用户选择。`machine_id` 是本地随机标识，不从硬件信息派生。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFile {
    pub enabled: bool,
    pub machine_id: String,
    pub consent_source: ConsentSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_run_notice_shown: Option<bool>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TelemetryDecision {
    #[serde(rename = "DO_NOT_TRACK")]
    DoNotTrack,
    #[serde(rename = "RUSTCODEGRAPH_TELEMETRY")]
    CodegraphTelemetry,
    #[serde(rename = "config")]
    Config,
    #[serde(rename = "default")]
    Default,
}

/// 对 CLI 展示和测试使用的“最终决策”，同时保留是谁覆盖了默认值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryStatus {
    pub enabled: bool,
    pub decided_by: TelemetryDecision,
    pub machine_id: Option<String>,
    pub config_path: String,
}

/// 使用计数的 JSONL 行。按天、事件名和客户端信息聚合，减少落盘和发送量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountLine {
    pub v: u32,
    pub d: String,
    pub k: UsageKind,
    pub n: String,
    pub c: u64,
    pub e: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cv: Option<String>,
}

/// 生命周期事件不做日级聚合，因为安装、卸载等事件本身就很稀疏。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventLine {
    pub v: u32,
    pub ev: LifecycleEvent,
    pub ts: String,
    pub props: HashMap<String, Value>,
}

/// 队列文件使用 untagged JSONL，方便旧版本行被逐行跳过而不破坏整批发送。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BufferLine {
    Count(CountLine),
    Event(EventLine),
}

/// 可注入时钟让测试能稳定覆盖 UTC 日切、陈旧 claim 恢复和时间戳格式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockInstant {
    pub iso: String,
    pub millis: i128,
}

impl ClockInstant {
    pub fn now() -> Self {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i128)
            .unwrap_or_default();
        Self::from_unix_millis(millis)
    }

    pub fn from_unix_millis(millis: i128) -> Self {
        Self {
            iso: iso_from_millis(millis),
            millis,
        }
    }

    pub fn from_iso(iso: &str) -> Self {
        Self {
            iso: iso.to_owned(),
            millis: parse_iso_millis(iso).unwrap_or_default(),
        }
    }

    fn day(&self) -> String {
        self.iso.chars().take(10).collect()
    }
}

#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: String,
    pub body: String,
    pub timeout_ms: u64,
}

pub type FetchResult = Result<(), String>;
pub type FetchImpl = Arc<dyn Fn(FetchRequest) -> FetchResult + Send + Sync + 'static>;
pub type NowFn = Arc<dyn Fn() -> ClockInstant + Send + Sync + 'static>;
pub type StderrFn = Arc<dyn Fn(&str) + Send + Sync + 'static>;

/// 依赖都从 options 注入，确保生产路径静默，测试路径可观测且不触网。
#[derive(Clone)]
pub struct TelemetryOptions {
    pub dir: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub fetch_impl: Option<FetchImpl>,
    pub now: Option<NowFn>,
    pub stderr: Option<StderrFn>,
    pub install_exit_hook: bool,
}

impl Default for TelemetryOptions {
    fn default() -> Self {
        Self {
            dir: None,
            env: env::vars().collect(),
            fetch_impl: None,
            now: None,
            stderr: None,
            install_exit_hook: true,
        }
    }
}

pub struct Telemetry {
    dir: PathBuf,
    env: HashMap<String, String>,
    fetch_impl: FetchImpl,
    now: NowFn,
    write_stderr: StderrFn,
    counts: HashMap<String, CountLine>,
    events: Vec<EventLine>,
    config_cache: Mutex<Option<Option<ConfigFile>>>,
    install_exit_hook: bool,
    exit_hook_installed: bool,
}

impl Telemetry {
    /// 默认目录放在用户级 `.rustcodegraph`，因为遥测选择跨项目生效。
    pub fn new(opts: TelemetryOptions) -> Self {
        Self {
            dir: opts.dir.unwrap_or_else(|| {
                crate::installer::targets::shared::home_dir().join(".rustcodegraph")
            }),
            env: opts.env,
            fetch_impl: opts.fetch_impl.unwrap_or_else(|| Arc::new(|_| Ok(()))),
            now: opts.now.unwrap_or_else(|| Arc::new(ClockInstant::now)),
            write_stderr: opts.stderr.unwrap_or_else(|| {
                Arc::new(|line| {
                    eprint!("{line}");
                })
            }),
            counts: HashMap::new(),
            events: Vec::new(),
            config_cache: Mutex::new(None),
            install_exit_hook: opts.install_exit_hook,
            exit_hook_installed: false,
        }
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.join("telemetry.json")
    }

    pub fn queue_path(&self) -> PathBuf {
        self.dir.join("telemetry-queue.jsonl")
    }

    /// Resolution order: DO_NOT_TRACK > RUSTCODEGRAPH_TELEMETRY > stored config > default on.
    /// 环境变量优先级必须高于配置文件，便于 CI 和一次性命令强制关闭。
    pub fn get_status(&self) -> TelemetryStatus {
        let config = self.read_config();
        let machine_id = config.as_ref().map(|config| config.machine_id.clone());
        if self.env.get("DO_NOT_TRACK").is_some_and(|value| {
            value != "0" && !value.eq_ignore_ascii_case("false") && !value.is_empty()
        }) {
            return TelemetryStatus {
                enabled: false,
                decided_by: TelemetryDecision::DoNotTrack,
                machine_id,
                config_path: self.config_path().to_string_lossy().into_owned(),
            };
        }
        if let Some(value) = self
            .env
            .get("RUSTCODEGRAPH_TELEMETRY")
            .filter(|value| !value.is_empty())
        {
            return TelemetryStatus {
                enabled: value != "0" && !value.eq_ignore_ascii_case("false"),
                decided_by: TelemetryDecision::CodegraphTelemetry,
                machine_id,
                config_path: self.config_path().to_string_lossy().into_owned(),
            };
        }
        if let Some(config) = config {
            return TelemetryStatus {
                enabled: config.enabled,
                decided_by: TelemetryDecision::Config,
                machine_id,
                config_path: self.config_path().to_string_lossy().into_owned(),
            };
        }
        TelemetryStatus {
            enabled: true,
            decided_by: TelemetryDecision::Default,
            machine_id,
            config_path: self.config_path().to_string_lossy().into_owned(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.get_status().enabled
    }

    pub fn set_enabled(&mut self, enabled: bool, source: ConsentSource) {
        let existing = self.read_config();
        self.write_config(ConfigFile {
            enabled,
            machine_id: existing
                .as_ref()
                .map(|config| config.machine_id.clone())
                .unwrap_or_else(machine_id),
            consent_source: source,
            first_run_notice_shown: Some(true),
            updated_at: (self.now)().iso,
        });
        if !enabled {
            // 关闭时立即丢弃内存和磁盘队列，避免用户选择之后还补发旧数据。
            self.counts.clear();
            self.events.clear();
            let _ = fs::remove_file(self.queue_path());
        }
    }

    pub fn has_stored_choice(&self) -> bool {
        self.read_config().is_some()
    }

    pub fn record_usage(
        &mut self,
        kind: UsageKind,
        name: &str,
        ok: bool,
        client: Option<ClientInfo>,
    ) {
        if !self.is_enabled() {
            return;
        }
        // key 中包含 day/client 维度；name/client 会截断，避免工具名异常膨胀队列。
        let day = self.utc_day();
        let cn = client
            .as_ref()
            .and_then(|client| client.name.as_ref())
            .map(|value| truncate(value, 64));
        let cv = client
            .as_ref()
            .and_then(|client| client.version.as_ref())
            .map(|value| truncate(value, 32));
        let key = [
            day.clone(),
            serde_json::to_string(&kind).unwrap_or_default(),
            name.to_owned(),
            cn.clone().unwrap_or_default(),
            cv.clone().unwrap_or_default(),
        ]
        .join("\0");
        let line = self.counts.entry(key).or_insert_with(|| CountLine {
            v: SCHEMA_VERSION,
            d: day,
            k: kind,
            n: truncate(name, 64),
            c: 0,
            e: 0,
            cn,
            cv,
        });
        line.c += 1;
        if !ok {
            line.e += 1;
        }
        self.ensure_exit_hook();
    }

    pub fn record_lifecycle(&mut self, event: LifecycleEvent, props: HashMap<String, Value>) {
        if !self.is_enabled() {
            return;
        }
        self.events.push(EventLine {
            v: SCHEMA_VERSION,
            ev: event,
            ts: (self.now)().iso,
            props,
        });
        self.ensure_exit_hook();
    }

    pub fn maybe_flush(&mut self) {
        self.flush_now();
    }

    pub fn flush_now(&mut self) {
        self.flush_now_with_timeout(DEFAULT_FLUSH_TIMEOUT_MS);
    }

    pub fn flush_now_with_timeout(&mut self, timeout_ms: u64) {
        if !self.is_enabled() {
            return;
        }
        // 发送顺序：先把内存增量落盘，再接管队列文件；这样崩溃最多留下可恢复 claim。
        self.persist_sync();
        self.recover_stale_claims();
        let Some((claim_path, lines)) = self.claim_queue() else {
            return;
        };
        let today = self.utc_day();
        let mut sendable = Vec::new();
        let mut keep = Vec::new();
        for line in lines {
            match &line {
                BufferLine::Event(_) => sendable.push(line),
                // 当天计数继续留在本地，以便进程内多次记录能合并到同一个日汇总。
                BufferLine::Count(count) if count.d < today => sendable.push(line),
                BufferLine::Count(_) => keep.push(line),
            }
        }
        let mut failed = Vec::new();
        if !sendable.is_empty() {
            self.first_run_notice();
            failed = self.send(&sendable, timeout_ms);
        }
        let mut back = failed;
        back.extend(keep);
        if !back.is_empty() {
            self.append_lines(&back);
        }
        let _ = fs::remove_file(claim_path);
    }

    pub fn start_interval(&mut self, _every_ms: u64) {
        if self.is_enabled() {
            self.maybe_flush();
        }
    }

    pub fn stop_interval(&mut self) {}

    /// Synchronously drains in-memory deltas to the JSONL queue.
    pub fn persist_sync(&mut self) {
        if self.counts.is_empty() && self.events.is_empty() {
            return;
        }
        let mut lines = self
            .counts
            .drain()
            .map(|(_, line)| BufferLine::Count(line))
            .collect::<Vec<_>>();
        lines.extend(self.events.drain(..).map(BufferLine::Event));
        if !self.is_enabled() {
            return;
        }
        self.append_lines(&lines);
    }

    fn utc_day(&self) -> String {
        (self.now)().day()
    }

    fn read_config(&self) -> Option<ConfigFile> {
        // `Option<Option<_>>` 区分“尚未读过”和“已确认没有有效配置”，减少热路径 IO。
        if let Ok(cache) = self.config_cache.lock()
            && let Some(cached) = &*cache
        {
            return cached.clone();
        }
        let parsed = fs::read_to_string(self.config_path())
            .ok()
            .and_then(|raw| serde_json::from_str::<ConfigFile>(&raw).ok())
            .filter(|config| !config.machine_id.is_empty());
        if let Ok(mut cache) = self.config_cache.lock() {
            *cache = Some(parsed.clone());
        }
        parsed
    }

    fn write_config(&self, config: ConfigFile) {
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let Ok(raw) = serde_json::to_string_pretty(&config) else {
            return;
        };
        if fs::write(self.config_path(), format!("{raw}\n")).is_ok()
            && let Ok(mut cache) = self.config_cache.lock()
        {
            *cache = Some(Some(config));
        }
    }

    fn first_run_notice(&self) {
        let config = self.read_config();
        if config
            .as_ref()
            .and_then(|config| config.first_run_notice_shown)
            .unwrap_or(false)
        {
            return;
        }
        if let Some(mut config) = config {
            config.first_run_notice_shown = Some(true);
            config.updated_at = (self.now)().iso;
            self.write_config(config);
        } else {
            // 默认开启也要先写 choice，再打印提示；后续 flush 不会重复打扰用户。
            self.write_config(ConfigFile {
                enabled: true,
                machine_id: machine_id(),
                consent_source: ConsentSource::DefaultNotice,
                first_run_notice_shown: Some(true),
                updated_at: (self.now)().iso,
            });
        }
        (self.write_stderr)(&format!(
            "rustcodegraph collects anonymous usage stats (no code, paths, or names) - \
             \"rustcodegraph telemetry off\" or RUSTCODEGRAPH_TELEMETRY=0 disables. Details: {TELEMETRY_DOCS}\n"
        ));
    }

    fn append_lines(&self, lines: &[BufferLine]) {
        if lines.is_empty() || fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let payload = lines
            .iter()
            .filter_map(|line| serde_json::to_string(line).ok())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let existing = fs::read_to_string(self.queue_path()).unwrap_or_default();
        let mut combined = existing + &payload;
        if combined.len() > MAX_BUFFER_BYTES {
            // 队列超限时保留最新尾部，并丢弃可能被截断的半行 JSON。
            combined = combined[combined.len() - MAX_BUFFER_BYTES..].to_owned();
            if let Some(index) = combined.find('\n') {
                combined = combined[index + 1..].to_owned();
            } else {
                combined.clear();
            }
        }
        let _ = fs::write(self.queue_path(), combined);
    }

    fn claim_queue(&self) -> Option<(PathBuf, Vec<BufferLine>)> {
        // rename 充当轻量 claim：同一台机器上的多个进程不会同时发送同一个队列文件。
        let claim_path = self.dir.join(format!(
            "telemetry-queue.sending.{}.jsonl",
            std::process::id()
        ));
        fs::rename(self.queue_path(), &claim_path).ok()?;
        let mut lines = Vec::new();
        if let Ok(raw) = fs::read_to_string(&claim_path) {
            for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
                let Ok(parsed) = serde_json::from_str::<BufferLine>(line) else {
                    continue;
                };
                if line_schema_version(&parsed) == SCHEMA_VERSION {
                    lines.push(parsed);
                }
            }
        }
        Some((claim_path, lines))
    }

    fn recover_stale_claims(&self) {
        let cutoff = (self.now)().millis - STALE_CLAIM_MS;
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("telemetry-queue.sending.") {
                continue;
            }
            let path = entry.path();
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            let modified_ms = modified
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as i128)
                .unwrap_or_default();
            if modified_ms >= cutoff {
                continue;
            }
            // 上次发送进程可能崩溃；陈旧 claim 重新并回主队列等待下一次 flush。
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let _ = fs::remove_file(&path);
            if content.trim().is_empty() {
                continue;
            }
            let mut normalized = content;
            if !normalized.ends_with('\n') {
                normalized.push('\n');
            }
            let _ = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.queue_path())
                .and_then(|mut file| {
                    use std::io::Write;
                    file.write_all(normalized.as_bytes())
                });
        }
    }

    fn send(&self, lines: &[BufferLine], timeout_ms: u64) -> Vec<BufferLine> {
        let Some(config) = self.read_config() else {
            return Vec::new();
        };
        let events = lines
            .iter()
            .map(|line| match line {
                BufferLine::Event(line) => serde_json::json!({
                    "event": line.ev,
                    "ts": line.ts,
                    "props": line.props,
                }),
                BufferLine::Count(line) => {
                    let mut props = serde_json::Map::from_iter([
                        ("kind".to_owned(), serde_json::json!(line.k)),
                        ("name".to_owned(), Value::String(line.n.clone())),
                        ("count".to_owned(), Value::from(line.c)),
                        ("error_count".to_owned(), Value::from(line.e)),
                    ]);
                    if let Some(client_name) = &line.cn {
                        props.insert("client_name".to_owned(), Value::String(client_name.clone()));
                    }
                    if let Some(client_version) = &line.cv {
                        props.insert(
                            "client_version".to_owned(),
                            Value::String(client_version.clone()),
                        );
                    }
                    serde_json::json!({
                        "event": "usage_rollup",
                        "ts": format!("{}T12:00:00.000Z", line.d),
                        "props": Value::Object(props),
                    })
                }
            })
            .collect::<Vec<_>>();
        let endpoint = self
            .env
            .get("RUSTCODEGRAPH_TELEMETRY_ENDPOINT")
            .filter(|value| !value.is_empty())
            .cloned()
            .unwrap_or_else(|| TELEMETRY_ENDPOINT.to_owned());
        for (chunk_index, chunk) in events.chunks(MAX_EVENTS_PER_REQUEST).enumerate() {
            let start = chunk_index * MAX_EVENTS_PER_REQUEST;
            let body = serde_json::json!({
                "machine_id": config.machine_id,
                "rustcodegraph_version": env!("CARGO_PKG_VERSION"),
                "os": env::consts::OS,
                "arch": env::consts::ARCH,
                "node_major": 0,
                "ci": self.env.get("CI").is_some_and(|value| {
                    !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
                }),
                "schema_version": SCHEMA_VERSION,
                "events": chunk,
            });
            self.debug(&format!("POST {endpoint} ({} events)", chunk.len()));
            let request = FetchRequest {
                url: endpoint.clone(),
                body: body.to_string(),
                timeout_ms,
            };
            if let Err(err) = self.fetch_with_timeout(request, timeout_ms) {
                self.debug(&format!("send failed: {err}"));
                // 已成功发送的 chunk 不再入队；失败 chunk 及之后的原始行原样重试。
                return lines[start..].to_vec();
            }
        }
        Vec::new()
    }

    fn fetch_with_timeout(&self, request: FetchRequest, timeout_ms: u64) -> FetchResult {
        // Rust 标准库无法取消阻塞 fetch；这里用接收超时保证 CLI 不被遥测拖住。
        let fetch = Arc::clone(&self.fetch_impl);
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send((fetch)(request));
        });
        rx.recv_timeout(Duration::from_millis(timeout_ms.max(1)))
            .unwrap_or_else(|_| Err("timeout".to_owned()))
    }

    fn ensure_exit_hook(&mut self) {
        if self.exit_hook_installed || !self.install_exit_hook {
            return;
        }
        self.exit_hook_installed = true;
    }

    fn debug(&self, msg: &str) {
        if self
            .env
            .get("RUSTCODEGRAPH_TELEMETRY_DEBUG")
            .is_some_and(|value| value == "1")
        {
            (self.write_stderr)(&format!("[rustcodegraph telemetry] {msg}\n"));
        }
    }
}

pub fn record_index_event(
    telemetry: &mut Telemetry,
    files_by_language: &HashMap<String, u64>,
    files_indexed: u64,
    duration_ms: u64,
    backend: &str,
) {
    // 索引事件只记录语言集合和粗粒度桶，刻意不包含文件名、路径或符号名。
    let languages = files_by_language
        .iter()
        .filter(|&(_lang, count)| *count > 0)
        .map(|(lang, _count)| Value::String(lang.clone()))
        .collect::<Vec<_>>();
    let props = HashMap::from([
        ("languages".to_owned(), Value::Array(languages)),
        (
            "file_count_bucket".to_owned(),
            Value::String(bucket_file_count(files_indexed).to_owned()),
        ),
        (
            "duration_bucket".to_owned(),
            Value::String(bucket_duration(duration_ms).to_owned()),
        ),
        (
            "sqlite_backend".to_owned(),
            Value::String(backend_kind(backend).to_owned()),
        ),
    ]);
    telemetry.record_lifecycle(LifecycleEvent::Index, props);
}

static SINGLETON: OnceLock<Mutex<Telemetry>> = OnceLock::new();

pub fn get_telemetry() -> &'static Mutex<Telemetry> {
    SINGLETON.get_or_init(|| Mutex::new(Telemetry::new(TelemetryOptions::default())))
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn line_schema_version(line: &BufferLine) -> u32 {
    match line {
        BufferLine::Count(line) => line.v,
        BufferLine::Event(line) => line.v,
    }
}

fn machine_id() -> String {
    // 生成 UUIDv4 形状的随机 ID；哈希输入只提供随机性，不携带可识别机器属性。
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut hasher = Sha256::new();
    hasher.update(now.to_le_bytes());
    hasher.update((std::process::id() as u64).to_le_bytes());
    hasher.update(counter.to_le_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn iso_from_millis(millis: i128) -> String {
    // 避免引入时间库依赖，使用 UTC civil date 算法做毫秒时间戳转换。
    let seconds = millis.div_euclid(1_000);
    let millisecond = millis.rem_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let second_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days as i64);
    let hour = second_of_day / 3_600;
    let minute = (second_of_day % 3_600) / 60;
    let second = second_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millisecond:03}Z")
}

fn parse_iso_millis(iso: &str) -> Option<i128> {
    if iso.len() < 24 {
        return None;
    }
    let year = iso.get(0..4)?.parse::<i32>().ok()?;
    let month = iso.get(5..7)?.parse::<u32>().ok()?;
    let day = iso.get(8..10)?.parse::<u32>().ok()?;
    let hour = iso.get(11..13)?.parse::<i128>().ok()?;
    let minute = iso.get(14..16)?.parse::<i128>().ok()?;
    let second = iso.get(17..19)?.parse::<i128>().ok()?;
    let millisecond = iso.get(20..23)?.parse::<i128>().ok()?;
    let days = days_from_civil(year, month, day) as i128;
    Some((((days * 24 + hour) * 60 + minute) * 60 + second) * 1_000 + millisecond)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u32, d as u32)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year as i64 - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
