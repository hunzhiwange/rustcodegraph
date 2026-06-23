//! End-to-end React Native event-channel synthesizer coverage.
//!
//! This is the Rust port of `__tests__/rn-event-channel.test.ts`.
//!
//! The TypeScript suite exercises the full `CodeGraph.indexAll()` pipeline.
//! Rust has the RN event synthesizer translated, but the current `CodeGraph`
//! facade does not yet run the full resolver/callback synthesis pipeline during
//! `index_all`, so these backend-dependent cases are recorded as ignored until
//! that wiring exists.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use rustcodegraph::directory::get_code_graph_dir;
use rustcodegraph::{CodeGraph, IndexOptions};

static RN_EVENT_DB_TEST_MUTEX: Mutex<()> = Mutex::new(());

fn with_serial_rn_event_db_test<T>(run: impl FnOnce() -> T) -> T {
    let _guard = RN_EVENT_DB_TEST_MUTEX
        .lock()
        .expect("RN event-channel DB test mutex should not be poisoned");
    run()
}

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("rn-event-fixture-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&root)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", root.display()));
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative_path: &str, content: &str) {
        fs::write(self.root.join(relative_path), content)
            .unwrap_or_else(|err| panic!("failed to write fixture {relative_path}: {err}"));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Debug)]
struct EventEdgeRow {
    source_name: String,
    source_language: String,
    target_name: String,
    target_kind: String,
    target_language: String,
    event: String,
}

fn index_and_read_event_edges(project_root: &Path, extra_where: Option<&str>) -> Vec<EventEdgeRow> {
    let mut cg = CodeGraph::init_sync(project_root).expect("failed to initialize CodeGraph");
    let result = cg.index_all(IndexOptions::default());
    assert!(result.success, "indexing failed: {:?}", result.errors);
    cg.close();

    let db_path = get_code_graph_dir(project_root).join("rustcodegraph.db");
    let conn = Connection::open(&db_path)
        .unwrap_or_else(|err| panic!("failed to open {}: {err}", db_path.display()));
    let sql = format!(
        "SELECT s.name source_name, s.language sl, t.name target_name, \
                t.kind target_kind, t.language tl, \
                json_extract(e.metadata,'$.event') event \
         FROM edges e \
         JOIN nodes s ON s.id = e.source \
         JOIN nodes t ON t.id = e.target \
         WHERE json_extract(e.metadata,'$.synthesizedBy') = 'rn-event-channel'{}",
        extra_where
            .map(|clause| format!(" AND {clause}"))
            .unwrap_or_default()
    );
    let mut stmt = conn
        .prepare(&sql)
        .unwrap_or_else(|err| panic!("failed to prepare event-edge query: {err}"));
    let rows = stmt
        .query_map([], |row| {
            Ok(EventEdgeRow {
                source_name: row.get("source_name")?,
                source_language: row.get("sl")?,
                target_name: row.get("target_name")?,
                target_kind: row.get("target_kind")?,
                target_language: row.get("tl")?,
                event: row.get("event")?,
            })
        })
        .expect("event-edge query should run")
        .collect::<Result<Vec<_>, _>>()
        .expect("event-edge rows should decode");

    rows
}

mod rn_event_channel_synthesizer {
    use super::*;

    #[test]
    fn synthesizes_an_edge_from_objc_sendeventwithname_to_js_addlistener_handler() {
        with_serial_rn_event_db_test(|| {
            let project = TempProject::new();
            // package.json so the RN detector / general resolver sees the project as RN.
            project.write(
                "package.json",
                "{\"name\":\"x\",\"dependencies\":{\"react-native\":\"^0.73\"}}",
            );
            project.write(
                "Emitter.m",
                r#"
@implementation Emitter
- (void)reportLocation {
    [self sendEventWithName:@"locationUpdate" body:@{}];
}
@end
"#,
            );
            project.write(
                "App.js",
                r#"
function onLocation(payload) {
    console.log(payload);
}
emitter.addListener('locationUpdate', onLocation);
"#,
            );

            let rows = index_and_read_event_edges(project.path(), None);
            assert!(!rows.is_empty(), "expected at least one RN event edge");

            // The edge should point from the ObjC method that emits to the JS handler.
            let edge = rows
                .iter()
                .find(|row| row.event == "locationUpdate")
                .expect("locationUpdate edge should be synthesized");
            assert_eq!(edge.source_language, "objc");
            assert_eq!(edge.target_language, "javascript");
            assert_eq!(edge.target_name, "onLocation");
        });
    }

    #[test]
    fn falls_back_to_enclosing_js_function_when_addlistener_handler_is_a_parameter_wrapper_api_pattern()
     {
        with_serial_rn_event_db_test(|| {
            let project = TempProject::new();
            // Matches the real RNFirebase shape: `messaging().onMessage(listener)`
            // is a subscribe-wrapper whose body does
            // `addListener('messaging_message_received', listener)` where `listener`
            // is the parameter -- not a globally-named symbol. Synthesizer should
            // still produce an edge, attributed to the enclosing wrapper function.
            project.write(
                "package.json",
                "{\"dependencies\":{\"react-native\":\"^0.73\"}}",
            );
            project.write(
                "Native.m",
                r#"
@implementation MyEmitter
- (void)pushMessage {
    [[Shared shared] sendEventWithName:@"messaging_message_received" body:@{}];
}
@end
"#,
            );
            project.write(
                "messaging.ts",
                r#"
import { NativeEventEmitter } from 'react-native';
const emitter = new NativeEventEmitter();
export function onMessage(listener: (m: any) => void) {
    return emitter.addListener('messaging_message_received', listener);
}
"#,
            );

            let rows = index_and_read_event_edges(project.path(), None);
            let edge = rows
                .iter()
                .find(|row| row.event == "messaging_message_received")
                .expect("messaging_message_received edge should be synthesized");

            // Target should be the wrapper function `onMessage` -- the enclosing
            // function of the addListener call, not a bareword named handler.
            assert_eq!(edge.target_name, "onMessage");
            assert!(
                ["function", "method"].contains(&edge.target_kind.as_str()),
                "expected target kind to be function or method, got {:?}",
                edge.target_kind
            );
        });
    }

    #[test]
    fn synthesizes_an_edge_from_a_java_sendevent_ctx_x_body_wrapper_to_a_js_handler() {
        with_serial_rn_event_db_test(|| {
            let project = TempProject::new();
            project.write(
                "package.json",
                "{\"dependencies\":{\"react-native\":\"^0.74.0\"}}",
            );
            // The literal event name lives in the WRAPPER CALL, not in `.emit` (whose
            // first arg is the `eventName` VARIABLE) -- the common react-native-device-info
            // shape that RN_JVM_EMIT_RE alone misses.
            project.write(
                "BatteryModule.java",
                r#"public class BatteryModule extends ReactContextBaseJavaModule {
  @Override public String getName() { return "BatteryModule"; }
  public void onBatteryChanged() {
    sendEvent(getReactApplicationContext(),
      "myWrapperBatteryEvent", null);
  }
  private void sendEvent(ReactContext ctx, String eventName, Object data) {
    ctx.getJSModule(DeviceEventManagerModule.RCTDeviceEventEmitter.class).emit(eventName, data);
  }
}
"#,
            );
            project.write(
                "index.ts",
                "function onBattery() {}\n\
             emitter.addListener('myWrapperBatteryEvent', onBattery);\n",
            );

            let rows = index_and_read_event_edges(
                project.path(),
                Some("json_extract(e.metadata,'$.event')='myWrapperBatteryEvent'"),
            );
            assert!(!rows.is_empty(), "expected myWrapperBatteryEvent edge");
            assert_eq!(rows[0].source_language, "java");
            assert_eq!(rows[0].source_name, "onBatteryChanged");
            assert_eq!(rows[0].target_name, "onBattery");
        });
    }
}
