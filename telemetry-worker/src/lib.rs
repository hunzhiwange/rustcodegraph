use serde::Serialize;
use serde_json::{Map, Value, json};
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_EVENTS_PER_BATCH: usize = 100;
const TEN_MINUTES_MS: i128 = 10 * 60_000;
const THIRTY_DAYS_MS: i128 = 30 * 86_400_000;

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const INFO_TEXT: &str = "codegraph anonymous-telemetry ingest.\n\n\
What gets collected (and what never does) is documented field-by-field:\n\
https://github.com/hunzhiwange/rustcodegraph/blob/main/docs/design/telemetry.md\n\
This endpoint's full source:\n\
https://github.com/hunzhiwange/rustcodegraph/tree/main/telemetry-worker\n\n\
Disable any time: codegraph telemetry off  |  CODEGRAPH_TELEMETRY=0  |  DO_NOT_TRACK=1\n";

#[derive(Clone, Copy)]
enum SanitizeRule {
    OneOf(&'static [&'static str]),
    Token(usize),
    Label(usize),
    TokenArray { max_items: usize, max_len: usize },
    NonNegInt(u64),
    Bool,
}

struct EventSpec {
    required: &'static [&'static str],
    props: &'static [(&'static str, SanitizeRule)],
}

#[derive(Debug, PartialEq, Eq)]
pub enum BatchError {
    BadRequest,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PostHogEvent {
    event: String,
    distinct_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
    properties: Map<String, Value>,
}

impl PostHogEvent {
    #[cfg(test)]
    fn properties(&self) -> &Map<String, Value> {
        &self.properties
    }
}

fn event_spec(event: &str) -> Option<EventSpec> {
    match event {
        "install" => Some(EventSpec {
            required: &["scope", "kind"],
            props: &[
                (
                    "targets",
                    SanitizeRule::TokenArray {
                        max_items: 12,
                        max_len: 24,
                    },
                ),
                ("scope", SanitizeRule::OneOf(&["local", "global"])),
                (
                    "kind",
                    SanitizeRule::OneOf(&["fresh", "upgrade", "reinstall"]),
                ),
                ("sqlite_backend", SanitizeRule::OneOf(&["native", "wasm"])),
            ],
        }),
        "index" => Some(EventSpec {
            required: &[],
            props: &[
                (
                    "languages",
                    SanitizeRule::TokenArray {
                        max_items: 32,
                        max_len: 24,
                    },
                ),
                (
                    "file_count_bucket",
                    SanitizeRule::OneOf(&["<100", "100-1k", "1k-10k", "10k+"]),
                ),
                (
                    "duration_bucket",
                    SanitizeRule::OneOf(&["<10s", "10-60s", "1-5m", "5m+"]),
                ),
                ("sqlite_backend", SanitizeRule::OneOf(&["native", "wasm"])),
            ],
        }),
        "usage_rollup" => Some(EventSpec {
            required: &["kind", "name", "count"],
            props: &[
                ("kind", SanitizeRule::OneOf(&["mcp_tool", "cli_command"])),
                ("name", SanitizeRule::Token(64)),
                ("count", SanitizeRule::NonNegInt(1_000_000)),
                ("error_count", SanitizeRule::NonNegInt(1_000_000)),
                ("client_name", SanitizeRule::Label(64)),
                ("client_version", SanitizeRule::Label(32)),
            ],
        }),
        "uninstall" => Some(EventSpec {
            required: &[],
            props: &[(
                "targets",
                SanitizeRule::TokenArray {
                    max_items: 12,
                    max_len: 24,
                },
            )],
        }),
        _ => None,
    }
}

fn envelope_props() -> &'static [(&'static str, SanitizeRule)] {
    &[
        ("codegraph_version", SanitizeRule::Token(32)),
        ("os", SanitizeRule::Token(16)),
        ("arch", SanitizeRule::Token(16)),
        ("node_major", SanitizeRule::NonNegInt(99)),
        ("ci", SanitizeRule::Bool),
        ("schema_version", SanitizeRule::NonNegInt(99)),
    ]
}

pub fn valid_body_len(content_length: Option<&str>) -> Result<(), u16> {
    let Some(content_length) = content_length else {
        return Err(411);
    };
    let Ok(len) = content_length.parse::<usize>() else {
        return Err(411);
    };
    if len == 0 {
        return Err(411);
    }
    if len > MAX_BODY_BYTES {
        return Err(413);
    }
    Ok(())
}

pub fn body_too_large(text: &str) -> bool {
    text.len() > MAX_BODY_BYTES
}

pub fn parse_batch(text: &str, now_ms: i128) -> Result<(String, Vec<PostHogEvent>), BatchError> {
    let parsed: Value = serde_json::from_str(text).map_err(|_| BatchError::BadRequest)?;
    let body = parsed.as_object().ok_or(BatchError::BadRequest)?;
    sanitize_batch(body, now_ms)
}

pub fn sanitize_batch(
    body: &Map<String, Value>,
    now_ms: i128,
) -> Result<(String, Vec<PostHogEvent>), BatchError> {
    let machine_id = body
        .get("machine_id")
        .and_then(Value::as_str)
        .filter(|id| is_uuidish(id))
        .ok_or(BatchError::BadRequest)?
        .to_owned();

    let mut common = Map::new();
    for (key, rule) in envelope_props() {
        if let Some(value) = body
            .get(*key)
            .and_then(|value| sanitize_value(value, *rule))
        {
            common.insert((*key).to_owned(), value);
        }
    }

    let events = body
        .get("events")
        .and_then(Value::as_array)
        .map(|events| events.iter().take(MAX_EVENTS_PER_BATCH))
        .into_iter()
        .flatten()
        .filter_map(|event| sanitize_event(event, &machine_id, &common, now_ms))
        .collect();

    Ok((machine_id, events))
}

fn sanitize_event(
    raw: &Value,
    machine_id: &str,
    common: &Map<String, Value>,
    now_ms: i128,
) -> Option<PostHogEvent> {
    let raw = raw.as_object()?;
    let event = raw.get("event")?.as_str()?;
    let spec = event_spec(event)?;
    let raw_props = raw.get("props").and_then(Value::as_object);

    let mut props = Map::new();
    for (key, rule) in spec.props {
        if let Some(value) = raw_props
            .and_then(|props| props.get(*key))
            .and_then(|value| sanitize_value(value, *rule))
        {
            props.insert((*key).to_owned(), value);
        }
    }
    if spec.required.iter().any(|key| !props.contains_key(*key)) {
        return None;
    }

    for (key, value) in common {
        props.insert(key.clone(), value.clone());
    }
    props.insert("$process_person_profile".to_owned(), Value::Bool(false));
    props.insert("$geoip_disable".to_owned(), Value::Bool(true));
    props.insert(
        "$lib".to_owned(),
        Value::String("codegraph-telemetry-worker".to_owned()),
    );

    Some(PostHogEvent {
        event: event.to_owned(),
        distinct_id: machine_id.to_owned(),
        timestamp: raw
            .get("ts")
            .and_then(Value::as_str)
            .and_then(|ts| clamp_timestamp(ts, now_ms)),
        properties: props,
    })
}

fn sanitize_value(value: &Value, rule: SanitizeRule) -> Option<Value> {
    match rule {
        SanitizeRule::OneOf(allowed) => value
            .as_str()
            .filter(|value| allowed.contains(value))
            .map(|value| Value::String(value.to_owned())),
        SanitizeRule::Token(max_len) => value
            .as_str()
            .filter(|value| valid_token(value, max_len))
            .map(|value| Value::String(value.to_owned())),
        SanitizeRule::Label(max_len) => value
            .as_str()
            .filter(|value| valid_label(value, max_len))
            .map(|value| Value::String(value.to_owned())),
        SanitizeRule::TokenArray { max_items, max_len } => {
            let values = value.as_array()?;
            if values.len() > max_items {
                return None;
            }
            let mut out = Vec::with_capacity(values.len());
            for value in values {
                let value = value.as_str()?;
                if !valid_token(value, max_len) {
                    return None;
                }
                out.push(Value::String(value.to_owned()));
            }
            Some(Value::Array(out))
        }
        SanitizeRule::NonNegInt(max) => sanitize_non_neg_int(value, max),
        SanitizeRule::Bool => value.as_bool().map(Value::Bool),
    }
}

fn sanitize_non_neg_int(value: &Value, max: u64) -> Option<Value> {
    if let Some(value) = value.as_u64() {
        return (value <= max).then(|| json!(value));
    }
    let value = value.as_f64()?;
    if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= max as f64 {
        return Some(json!(value as u64));
    }
    None
}

fn valid_token(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'+' | b'-'))
}

fn valid_label(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'_' | b'.' | b':' | b'+' | b'/' | b' ' | b'@' | b'(' | b')' | b'-'
                )
        })
}

fn is_uuidish(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    value.bytes().enumerate().all(|(idx, byte)| match idx {
        8 | 13 | 18 | 23 => byte == b'-',
        _ => byte.is_ascii_hexdigit(),
    })
}

fn clamp_timestamp(value: &str, now_ms: i128) -> Option<String> {
    let parsed = OffsetDateTime::parse(value, &Rfc3339).ok()?;
    let timestamp_ms = parsed.unix_timestamp_nanos() / 1_000_000;
    if timestamp_ms > now_ms + TEN_MINUTES_MS || timestamp_ms < now_ms - THIRTY_DAYS_MS {
        return None;
    }
    Some(format_iso_millis(parsed.to_offset(UtcOffset::UTC)))
}

fn format_iso_millis(value: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        value.year(),
        u8::from(value.month()),
        value.day(),
        value.hour(),
        value.minute(),
        value.second(),
        value.millisecond()
    )
}

#[cfg(target_arch = "wasm32")]
mod worker_entry {
    use super::{INFO_TEXT, PostHogEvent, body_too_large, parse_batch, valid_body_len};
    use js_sys::Date;
    use serde_json::json;
    use wasm_bindgen::JsValue;
    use worker::{
        AbortSignal, Env, Fetch, Headers, Method, Request, RequestInit, Response, Result,
        console_error, event,
    };

    #[event(fetch)]
    pub async fn main(request: Request, env: Env, ctx: worker::Context) -> Result<Response> {
        console_error_panic_hook::set_once();
        match handle(request, env, ctx).await {
            Ok(response) => Ok(response),
            Err(err) => {
                console_error!(
                    "{}",
                    json!({ "msg": "unhandled error", "err": err.to_string() })
                );
                text_response("internal error\n", 500)
            }
        }
    }

    async fn handle(mut request: Request, env: Env, ctx: worker::Context) -> Result<Response> {
        let path = request.path();
        if request.method() == Method::Get && path == "/" {
            return text_response(INFO_TEXT, 200);
        }
        if path != "/v1/events" {
            return text_response("not found\n", 404);
        }
        if request.method() != Method::Post {
            let headers = Headers::new();
            headers.set("allow", "POST")?;
            return Ok(Response::builder()
                .with_status(405)
                .with_headers(headers)
                .fixed(b"method not allowed\n".to_vec()));
        }

        match valid_body_len(request.headers().get("content-length")?.as_deref()) {
            Ok(()) => {}
            Err(status) => {
                let body = if status == 413 {
                    "payload too large\n"
                } else {
                    "length required\n"
                };
                return text_response(body, status);
            }
        }

        let text = request.text().await.map_err(worker::Error::from)?;
        if body_too_large(&text) {
            return text_response("payload too large\n", 413);
        }

        let (machine_id, batch) = match parse_batch(&text, Date::now() as i128) {
            Ok(batch) => batch,
            Err(_) => return text_response("bad request\n", 400),
        };

        if let Ok(rate_limiter) = env.rate_limiter("MACHINE_RATE_LIMITER") {
            match rate_limiter.limit(machine_id).await {
                Ok(outcome) if !outcome.success => return text_response("rate limited\n", 429),
                Ok(_) => {}
                Err(err) => console_error!(
                    "{}",
                    json!({ "msg": "rate limiter unavailable", "err": err.to_string() })
                ),
            }
        }

        if !batch.is_empty() {
            ctx.wait_until(forward_to_posthog(env, batch));
        }

        Ok(Response::builder().with_status(204).empty())
    }

    async fn forward_to_posthog(env: Env, batch: Vec<PostHogEvent>) {
        let result = async {
            let host = env
                .var("POSTHOG_HOST")
                .map(|value| value.to_string())
                .unwrap_or_else(|_| "https://us.i.posthog.com".to_owned());
            let key = env.secret("POSTHOG_KEY")?.to_string();
            let body = serde_json::to_string(&json!({ "api_key": key, "batch": batch }))
                .map_err(|err| worker::Error::from(err.to_string()))?;

            let headers = Headers::new();
            headers.set("content-type", "application/json")?;
            let mut init = RequestInit::new();
            init.with_method(Method::Post)
                .with_headers(headers)
                .with_body(Some(JsValue::from_str(&body)));
            let request = Request::new_with_init(&format!("{host}/batch/"), &init)?;
            let signal = AbortSignal::from(web_sys::AbortSignal::timeout_with_u32(5000));
            let response = Fetch::Request(request).send_with_signal(&signal).await?;
            let status = response.status_code();
            if !(200..300).contains(&status) {
                console_error!(
                    "{}",
                    json!({
                        "msg": "posthog forward failed",
                        "status": status,
                        "events": batch.len()
                    })
                );
            }
            Ok::<(), worker::Error>(())
        }
        .await;

        if let Err(err) = result {
            console_error!(
                "{}",
                json!({ "msg": "posthog forward error", "err": err.to_string() })
            );
        }
    }

    fn text_response(body: &str, status: u16) -> Result<Response> {
        let headers = Headers::new();
        headers.set("content-type", "text/plain; charset=utf-8")?;
        Ok(Response::builder()
            .with_status(status)
            .with_headers(headers)
            .fixed(body.as_bytes().to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_MS: i128 = 1_718_000_000_000;
    const MACHINE_ID: &str = "00000000-0000-4000-8000-000000000000";

    fn batch(input: Value) -> Vec<PostHogEvent> {
        let body = input.as_object().expect("test batch should be an object");
        sanitize_batch(body, NOW_MS)
            .expect("batch should be valid")
            .1
    }

    #[test]
    fn sanitizes_valid_usage_rollup_and_common_envelope() {
        let events = batch(json!({
            "machine_id": MACHINE_ID,
            "codegraph_version": "1.0.0",
            "os": "darwin",
            "arch": "arm64",
            "node_major": 22,
            "ci": false,
            "schema_version": 1,
            "events": [{
                "event": "usage_rollup",
                "ts": "2024-06-09T20:26:40Z",
                "props": {
                    "kind": "mcp_tool",
                    "name": "codegraph_explore",
                    "count": 12,
                    "error_count": 0,
                    "client_name": "Claude Code",
                    "client_version": "2.1",
                    "unknown": "stripped"
                }
            }]
        }));

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.event, "usage_rollup");
        assert_eq!(event.distinct_id, MACHINE_ID);
        assert_eq!(event.timestamp.as_deref(), Some("2024-06-09T20:26:40.000Z"));
        let props = event.properties();
        assert_eq!(props.get("kind"), Some(&json!("mcp_tool")));
        assert_eq!(props.get("name"), Some(&json!("codegraph_explore")));
        assert_eq!(props.get("count"), Some(&json!(12)));
        assert_eq!(props.get("codegraph_version"), Some(&json!("1.0.0")));
        assert_eq!(props.get("$process_person_profile"), Some(&json!(false)));
        assert_eq!(props.get("$geoip_disable"), Some(&json!(true)));
        assert!(!props.contains_key("unknown"));
    }

    #[test]
    fn drops_events_missing_required_properties() {
        let events = batch(json!({
            "machine_id": MACHINE_ID,
            "events": [
                { "event": "install", "props": { "scope": "local" } },
                { "event": "usage_rollup", "props": { "kind": "mcp_tool", "count": 1 } }
            ]
        }));

        assert!(events.is_empty());
    }

    #[test]
    fn drops_unknown_events_and_strips_unknown_properties() {
        let events = batch(json!({
            "machine_id": MACHINE_ID,
            "events": [
                { "event": "mystery", "props": { "kind": "mcp_tool", "name": "x", "count": 1 } },
                { "event": "uninstall", "props": { "targets": ["codex"], "repo": "/tmp/project" } }
            ]
        }));

        assert_eq!(events.len(), 1);
        let props = events[0].properties();
        assert_eq!(props.get("targets"), Some(&json!(["codex"])));
        assert!(!props.contains_key("repo"));
    }

    #[test]
    fn rejects_invalid_machine_ids_before_rate_limiting_key_is_used() {
        let body = json!({ "machine_id": "not-a-uuid", "events": [] });
        let body = body.as_object().unwrap();

        assert_eq!(sanitize_batch(body, NOW_MS), Err(BatchError::BadRequest));
    }

    #[test]
    fn exposes_machine_id_as_the_rate_limit_key() {
        let body = json!({
            "machine_id": MACHINE_ID,
            "events": [{ "event": "uninstall", "props": { "targets": ["codex"] } }]
        });
        let body = body.as_object().unwrap();

        let (rate_limit_key, events) = sanitize_batch(body, NOW_MS).unwrap();
        assert_eq!(rate_limit_key, MACHINE_ID);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn never_preserves_ip_like_fields() {
        let events = batch(json!({
            "machine_id": MACHINE_ID,
            "ip": "203.0.113.42",
            "CF-Connecting-IP": "203.0.113.42",
            "events": [{
                "event": "usage_rollup",
                "props": {
                    "kind": "mcp_tool",
                    "name": "codegraph_node",
                    "count": 1,
                    "client_ip": "203.0.113.42"
                }
            }]
        }));

        let props = events[0].properties();
        assert!(!props.contains_key("ip"));
        assert!(!props.contains_key("CF-Connecting-IP"));
        assert!(!props.contains_key("client_ip"));
        assert_eq!(props.get("$geoip_disable"), Some(&json!(true)));
    }

    #[test]
    fn enforces_body_and_batch_limits() {
        assert_eq!(valid_body_len(None), Err(411));
        assert_eq!(valid_body_len(Some("0")), Err(411));
        assert_eq!(valid_body_len(Some("65537")), Err(413));
        assert!(valid_body_len(Some("65536")).is_ok());
        assert!(body_too_large(&"x".repeat(65 * 1024)));

        let events = (0..105)
            .map(|_| json!({ "event": "uninstall", "props": { "targets": ["codex"] } }))
            .collect::<Vec<_>>();
        let events = batch(json!({ "machine_id": MACHINE_ID, "events": events }));
        assert_eq!(events.len(), 100);
    }
}
