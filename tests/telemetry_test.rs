use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use rustcodegraph::telemetry::index::{
    ClientInfo, ClockInstant, ConsentSource, FetchImpl, FetchRequest, LifecycleEvent,
    TELEMETRY_ENDPOINT, Telemetry, TelemetryDecision, TelemetryOptions, UsageKind, get_telemetry,
};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
struct FetchCall {
    url: String,
    body: Value,
}

fn mock_fetch(calls: Arc<Mutex<Vec<FetchCall>>>, fail: bool) -> FetchImpl {
    Arc::new(move |request: FetchRequest| {
        if fail {
            return Err("network down".to_owned());
        }
        let body = serde_json::from_str(&request.body)
            .unwrap_or_else(|err| panic!("fetch body should be JSON: {err}\n{}", request.body));
        calls
            .lock()
            .expect("calls lock should not be poisoned")
            .push(FetchCall {
                url: request.url,
                body,
            });
        Ok(())
    })
}

fn props(pairs: Vec<(&str, Value)>) -> HashMap<String, Value> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{unique}-{counter}",
            std::process::id()
        ));
        fs::create_dir_all(&path)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", path.display()));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct TelemetryFixture {
    dir: TempDir,
    calls: Arc<Mutex<Vec<FetchCall>>>,
    stderr_lines: Arc<Mutex<Vec<String>>>,
    now: Arc<Mutex<ClockInstant>>,
}

impl TelemetryFixture {
    fn new() -> Self {
        Self {
            dir: TempDir::new("codegraph-telemetry"),
            calls: Arc::new(Mutex::new(Vec::new())),
            stderr_lines: Arc::new(Mutex::new(Vec::new())),
            now: Arc::new(Mutex::new(ClockInstant::from_iso(
                "2026-06-12T08:00:00.000Z",
            ))),
        }
    }

    fn make(&self) -> Telemetry {
        self.make_with(HashMap::new(), mock_fetch(Arc::clone(&self.calls), false))
    }

    fn make_with_env(&self, env: HashMap<String, String>) -> Telemetry {
        self.make_with(env, mock_fetch(Arc::clone(&self.calls), false))
    }

    fn make_with_fetch(&self, fetch_impl: FetchImpl) -> Telemetry {
        self.make_with(HashMap::new(), fetch_impl)
    }

    fn make_with(&self, env: HashMap<String, String>, fetch_impl: FetchImpl) -> Telemetry {
        let now = Arc::clone(&self.now);
        let stderr_lines = Arc::clone(&self.stderr_lines);
        Telemetry::new(TelemetryOptions {
            dir: Some(self.dir.path().to_path_buf()),
            fetch_impl: Some(fetch_impl),
            now: Some(Arc::new(move || {
                now.lock()
                    .expect("clock lock should not be poisoned")
                    .clone()
            })),
            env,
            stderr: Some(Arc::new(move |line| {
                stderr_lines
                    .lock()
                    .expect("stderr lock should not be poisoned")
                    .push(line.to_owned());
            })),
            install_exit_hook: false,
        })
    }

    fn set_now(&self, iso: &str) {
        *self.now.lock().expect("clock lock should not be poisoned") = ClockInstant::from_iso(iso);
    }

    fn calls(&self) -> Vec<FetchCall> {
        self.calls
            .lock()
            .expect("calls lock should not be poisoned")
            .clone()
    }

    fn stderr_lines(&self) -> Vec<String> {
        self.stderr_lines
            .lock()
            .expect("stderr lock should not be poisoned")
            .clone()
    }

    fn now_millis(&self) -> i128 {
        self.now
            .lock()
            .expect("clock lock should not be poisoned")
            .millis
    }
}

fn events(call: &FetchCall) -> &[Value] {
    call.body["events"]
        .as_array()
        .expect("events should be an array")
}

mod telemetry {
    use super::*;

    mod consent_precedence {
        use super::*;

        #[test]
        fn defaults_to_enabled_when_nothing_decides_otherwise() {
            let fixture = TelemetryFixture::new();
            let t = fixture.make();

            let status = t.get_status();
            assert!(status.enabled);
            assert_eq!(status.decided_by, TelemetryDecision::Default);
            assert_eq!(status.machine_id, None);
        }

        #[test]
        fn do_not_track_beats_everything_including_a_forced_on_env_and_config() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make_with_env(HashMap::from([
                ("DO_NOT_TRACK".to_owned(), "1".to_owned()),
                ("RUSTCODEGRAPH_TELEMETRY".to_owned(), "1".to_owned()),
            ]));

            t.set_enabled(true, ConsentSource::Cli);

            let status = t.get_status();
            assert!(!status.enabled);
            assert_eq!(status.decided_by, TelemetryDecision::DoNotTrack);
        }

        #[test]
        fn codegraph_telemetry_env_beats_the_stored_config_in_both_directions() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make_with_env(HashMap::from([(
                "RUSTCODEGRAPH_TELEMETRY".to_owned(),
                "0".to_owned(),
            )]));

            t.set_enabled(true, ConsentSource::Cli);

            let status = t.get_status();
            assert!(!status.enabled);
            assert_eq!(status.decided_by, TelemetryDecision::CodegraphTelemetry);

            let mut t2 = fixture.make_with_env(HashMap::from([(
                "RUSTCODEGRAPH_TELEMETRY".to_owned(),
                "1".to_owned(),
            )]));
            t2.set_enabled(false, ConsentSource::Cli);

            let status = t2.get_status();
            assert!(status.enabled);
            assert_eq!(status.decided_by, TelemetryDecision::CodegraphTelemetry);
        }

        #[test]
        fn stored_config_decides_when_no_env_is_set() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make();

            t.set_enabled(false, ConsentSource::Installer);

            let status = t.get_status();
            assert!(!status.enabled);
            assert_eq!(status.decided_by, TelemetryDecision::Config);
        }
    }

    mod off_is_off {
        use super::*;

        #[test]
        fn disabled_records_nothing_sends_nothing_creates_no_files() {
            let fixture = TelemetryFixture::new();
            let fetch_spy = mock_fetch(Arc::clone(&fixture.calls), false);
            let mut t = fixture.make_with(
                HashMap::from([("RUSTCODEGRAPH_TELEMETRY".to_owned(), "0".to_owned())]),
                fetch_spy,
            );

            t.record_usage(UsageKind::McpTool, "codegraph_explore", true, None);
            t.record_lifecycle(
                LifecycleEvent::Install,
                props(vec![("scope", json!("local")), ("kind", json!("fresh"))]),
            );
            t.persist_sync();
            t.flush_now();

            assert!(fixture.calls().is_empty());
            assert!(!t.config_path().exists());
            assert!(!t.queue_path().exists());
            assert_eq!(fixture.stderr_lines(), Vec::<String>::new());
        }

        #[test]
        fn turning_telemetry_off_deletes_buffered_unsent_data() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make();

            t.record_usage(UsageKind::CliCommand, "init", true, None);
            t.persist_sync();
            assert!(t.queue_path().exists());

            t.set_enabled(false, ConsentSource::Cli);

            assert!(!t.queue_path().exists());
        }
    }

    mod first_run_notice_and_machine_id {
        use super::*;

        #[test]
        fn recording_only_buffers_no_notice_no_config_until_something_is_sent() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make();

            t.record_usage(UsageKind::McpTool, "codegraph_explore", true, None);
            t.record_usage(UsageKind::McpTool, "codegraph_node", true, None);

            assert_eq!(fixture.stderr_lines(), Vec::<String>::new());
            assert!(!t.config_path().exists());

            t.flush_now();

            assert_eq!(fixture.stderr_lines(), Vec::<String>::new());
            assert!(fixture.calls().is_empty());
        }

        #[test]
        fn prints_the_notice_exactly_once_before_the_first_actual_send() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make();

            t.record_lifecycle(
                LifecycleEvent::Index,
                props(vec![("languages", json!(["go"]))]),
            );
            t.flush_now();
            t.record_lifecycle(
                LifecycleEvent::Index,
                props(vec![("languages", json!(["rust"]))]),
            );
            t.flush_now();

            assert_eq!(fixture.calls().len(), 2);
            let stderr_lines = fixture.stderr_lines();
            assert_eq!(stderr_lines.len(), 1);
            assert!(stderr_lines[0].contains("rustcodegraph telemetry off"));
            assert!(stderr_lines[0].contains("RUSTCODEGRAPH_TELEMETRY=0"));

            let config: Value = serde_json::from_str(
                &fs::read_to_string(t.config_path()).expect("config should be readable"),
            )
            .expect("config should be JSON");
            let machine_id = config["machine_id"]
                .as_str()
                .expect("machine_id should be a string");
            assert!(Regex::new(r"^[0-9a-f-]{36}$").unwrap().is_match(machine_id));
            assert_eq!(config["consent_source"], json!("default-notice"));
        }

        #[test]
        fn keeps_the_machine_id_stable_across_instances_and_explicit_toggles() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make();

            t.record_lifecycle(
                LifecycleEvent::Install,
                props(vec![("scope", json!("local")), ("kind", json!("fresh"))]),
            );
            t.flush_now();
            let id1 = t.get_status().machine_id;
            assert!(id1.is_some());

            let mut t2 = fixture.make();
            t2.set_enabled(true, ConsentSource::Cli);

            assert_eq!(t2.get_status().machine_id, id1);
        }

        #[test]
        fn an_explicit_installer_choice_suppresses_the_notice() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make();

            t.set_enabled(true, ConsentSource::Installer);
            t.record_lifecycle(
                LifecycleEvent::Install,
                props(vec![("scope", json!("local")), ("kind", json!("fresh"))]),
            );
            t.flush_now();

            assert_eq!(fixture.calls().len(), 1);
            assert_eq!(fixture.stderr_lines(), Vec::<String>::new());
        }
    }

    mod rollups_and_sending {
        use super::*;

        #[test]
        fn aggregates_per_day_kind_name_client_and_sends_only_completed_days() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make();
            let client = ClientInfo {
                name: Some("Claude Code".to_owned()),
                version: Some("2.1".to_owned()),
            };

            t.record_usage(
                UsageKind::McpTool,
                "codegraph_explore",
                true,
                Some(client.clone()),
            );
            t.record_usage(
                UsageKind::McpTool,
                "codegraph_explore",
                false,
                Some(client.clone()),
            );
            t.record_usage(UsageKind::McpTool, "codegraph_explore", true, Some(client));
            t.record_usage(UsageKind::CliCommand, "query", true, None);

            t.flush_now();
            assert!(fixture.calls().is_empty());

            fixture.set_now("2026-06-13T08:00:00.000Z");
            t.record_usage(UsageKind::CliCommand, "status", true, None);
            t.flush_now();

            let calls = fixture.calls();
            assert_eq!(calls.len(), 1);
            let body = &calls[0].body;
            assert_eq!(body["machine_id"], json!(t.get_status().machine_id));
            assert_eq!(body["schema_version"], json!(1));
            assert_eq!(events(&calls[0]).len(), 2);
            let explore = events(&calls[0])
                .iter()
                .find(|event| event["props"]["name"] == json!("codegraph_explore"))
                .expect("explore rollup should be present");
            assert_eq!(explore["event"], json!("usage_rollup"));
            assert_eq!(explore["ts"], json!("2026-06-12T12:00:00.000Z"));
            assert_eq!(explore["props"]["kind"], json!("mcp_tool"));
            assert_eq!(explore["props"]["count"], json!(3));
            assert_eq!(explore["props"]["error_count"], json!(1));
            assert_eq!(explore["props"]["client_name"], json!("Claude Code"));
            assert_eq!(explore["props"]["client_version"], json!("2.1"));
            assert!(
                fs::read_to_string(t.queue_path())
                    .expect("queue should be readable")
                    .contains("\"status\"")
            );
        }

        #[test]
        fn lifecycle_events_send_on_the_next_flush_regardless_of_day() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make();

            t.record_lifecycle(
                LifecycleEvent::Install,
                props(vec![
                    ("targets", json!(["claude"])),
                    ("scope", json!("local")),
                    ("kind", json!("fresh")),
                ]),
            );
            t.flush_now();

            let calls = fixture.calls();
            assert_eq!(calls.len(), 1);
            assert_eq!(events(&calls[0])[0]["event"], json!("install"));
            assert_eq!(events(&calls[0])[0]["props"]["scope"], json!("local"));
            assert_eq!(events(&calls[0])[0]["props"]["kind"], json!("fresh"));
        }

        #[test]
        fn uses_the_production_endpoint_by_default_and_honors_the_env_override() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make();

            t.record_lifecycle(LifecycleEvent::Uninstall, HashMap::new());
            t.flush_now();
            assert_eq!(fixture.calls()[0].url, TELEMETRY_ENDPOINT);

            let mut t2 = fixture.make_with_env(HashMap::from([(
                "RUSTCODEGRAPH_TELEMETRY_ENDPOINT".to_owned(),
                "http://localhost:9999/v1/events".to_owned(),
            )]));
            t2.record_lifecycle(LifecycleEvent::Uninstall, HashMap::new());
            t2.flush_now();

            assert_eq!(fixture.calls()[1].url, "http://localhost:9999/v1/events");
        }

        #[test]
        fn re_queues_on_network_failure_and_delivers_on_the_next_flush() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make_with_fetch(mock_fetch(Arc::clone(&fixture.calls), true));

            t.record_lifecycle(
                LifecycleEvent::Install,
                props(vec![("scope", json!("global")), ("kind", json!("upgrade"))]),
            );
            t.flush_now();

            assert!(fixture.calls().is_empty());
            assert!(
                fs::read_to_string(t.queue_path())
                    .expect("queue should be readable")
                    .contains("\"install\"")
            );
            let claim_files = fs::read_dir(fixture.dir.path())
                .expect("telemetry dir should be readable")
                .flatten()
                .filter(|entry| entry.file_name().to_string_lossy().contains(".sending."))
                .count();
            assert_eq!(claim_files, 0);

            let mut t2 = fixture.make();
            t2.flush_now();

            assert_eq!(fixture.calls().len(), 1);
            assert!(!t2.queue_path().exists());
        }

        #[test]
        fn a_hung_endpoint_is_bounded_by_the_flush_timeout() {
            let fixture = TelemetryFixture::new();
            let hanging_fetch: FetchImpl = Arc::new(|request| {
                std::thread::sleep(Duration::from_millis(request.timeout_ms + 5_000));
                Ok(())
            });
            let mut t = fixture.make_with_fetch(hanging_fetch);

            t.record_lifecycle(
                LifecycleEvent::Install,
                props(vec![("scope", json!("local")), ("kind", json!("fresh"))]),
            );
            let started = Instant::now();
            t.flush_now_with_timeout(100);

            assert!(started.elapsed() < Duration::from_secs(2));
            assert!(
                fs::read_to_string(t.queue_path())
                    .expect("queue should be readable")
                    .contains("\"install\"")
            );
        }
    }

    mod buffer_robustness {
        use super::*;

        #[test]
        fn caps_the_queue_and_drops_oldest_lines_without_leaving_partial_json() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make();
            let targets = (0..50)
                .map(|i| Value::String(format!("agent-{i}")))
                .collect::<Vec<_>>();

            for i in 0..600 {
                t.record_lifecycle(
                    LifecycleEvent::Install,
                    props(vec![
                        ("targets", Value::Array(targets.clone())),
                        ("kind", json!("fresh")),
                        ("scope", json!("local")),
                        ("seq", json!(i)),
                    ]),
                );
                t.persist_sync();
            }

            let content = fs::read_to_string(t.queue_path()).expect("queue should be readable");
            assert!(content.len() <= 256 * 1024);
            let first = content
                .lines()
                .next()
                .expect("queue should have a first line");
            let first_json: Value =
                serde_json::from_str(first).expect("first line should be valid JSON");
            assert!(
                first_json["props"]["seq"]
                    .as_i64()
                    .expect("seq should be an integer")
                    > 0
            );
        }

        #[test]
        fn skips_corrupt_lines_and_still_delivers_the_valid_ones() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make();

            t.record_lifecycle(
                LifecycleEvent::Index,
                props(vec![("languages", json!(["typescript"]))]),
            );
            t.persist_sync();
            fs::OpenOptions::new()
                .append(true)
                .open(t.queue_path())
                .expect("queue should open for append")
                .write_all(b"NOT JSON{{{\n")
                .expect("corrupt line should be appended");
            t.flush_now();

            let calls = fixture.calls();
            assert_eq!(calls.len(), 1);
            assert_eq!(events(&calls[0]).len(), 1);
        }

        #[test]
        fn merges_back_stale_claim_files_from_a_crashed_sender() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make();
            let stale = fixture
                .dir
                .path()
                .join("telemetry-queue.sending.99999.jsonl");
            fs::create_dir_all(fixture.dir.path()).expect("telemetry dir should be created");
            fs::write(
                &stale,
                format!(
                    "{}\n",
                    json!({"v": 1, "ev": "uninstall", "ts": "2026-06-11T00:00:00.000Z", "props": {}})
                ),
            )
            .expect("stale claim should be written");
            let old =
                UNIX_EPOCH + Duration::from_millis((fixture.now_millis() - 2 * 60 * 60_000) as u64);
            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&stale)
                .expect("stale claim should open");
            file.set_times(fs::FileTimes::new().set_accessed(old).set_modified(old))
                .expect("stale claim mtime should be set");

            t.set_enabled(true, ConsentSource::Cli);
            t.flush_now();

            assert!(!stale.exists());
            let calls = fixture.calls();
            assert_eq!(calls.len(), 1);
            assert_eq!(events(&calls[0])[0]["event"], json!("uninstall"));
        }
    }

    mod protocol_safety {
        use super::*;

        #[test]
        fn never_writes_to_stdout() {
            let fixture = TelemetryFixture::new();
            let mut t = fixture.make_with_env(HashMap::from([(
                "RUSTCODEGRAPH_TELEMETRY_DEBUG".to_owned(),
                "1".to_owned(),
            )]));

            t.record_usage(UsageKind::McpTool, "codegraph_explore", true, None);
            t.record_lifecycle(
                LifecycleEvent::Install,
                props(vec![("scope", json!("local")), ("kind", json!("fresh"))]),
            );
            t.flush_now();

            assert_eq!(fixture.calls().len(), 1);
            assert!(
                fixture
                    .stderr_lines()
                    .iter()
                    .any(|line| line.contains("[rustcodegraph telemetry] POST"))
            );
        }
    }

    #[test]
    fn get_telemetry_returns_a_process_wide_singleton() {
        let a = get_telemetry() as *const _;
        let b = get_telemetry() as *const _;

        assert_eq!(a, b);
    }
}
