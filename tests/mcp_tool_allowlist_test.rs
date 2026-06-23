//! `RUSTCODEGRAPH_MCP_TOOLS` allowlist.

use std::env;
use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard, OnceLock};

use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use serde_json::{Map, Value, json};

const ENV: &str = "RUSTCODEGRAPH_MCP_TOOLS";

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    original: Option<OsString>,
}

impl EnvGuard {
    fn unset() -> Self {
        let guard = Self::new();
        unsafe {
            env::remove_var(ENV);
        }
        guard
    }

    fn set(value: &str) -> Self {
        let guard = Self::new();
        unsafe {
            env::set_var(ENV, value);
        }
        guard
    }

    fn new() -> Self {
        let lock = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("mcp tool allowlist env lock should not be poisoned");
        Self {
            _lock: lock,
            original: env::var_os(ENV),
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(value) = &self.original {
                env::set_var(ENV, value);
            } else {
                env::remove_var(ENV);
            }
        }
    }
}

fn listed() -> Vec<String> {
    let mut names = ToolHandler::new(false)
        .get_tools()
        .into_iter()
        .map(|tool| tool.name)
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn names(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|name| (*name).to_string()).collect()
}

fn args(items: &[(&str, Value)]) -> Map<String, Value> {
    items
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect()
}

fn first_text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

mod codegraph_mcp_tools_allowlist {
    use super::*;

    #[test]
    fn exposes_the_default_4_tool_surface_when_unset() {
        let _env = EnvGuard::unset();

        // The default set (see DEFAULT_MCP_TOOLS): explore + node are the
        // validated workhorses, search the cheap lookup, callers the one
        // irreplaceable enumerator. callees/impact/files/status stay defined
        // and executable but unlisted - impact appeared in ZERO recorded runs.
        assert_eq!(
            listed(),
            names(&[
                "rustcodegraph_callers",
                "rustcodegraph_explore",
                "rustcodegraph_node",
                "rustcodegraph_search",
            ])
        );
    }

    #[test]
    fn re_enables_an_unlisted_tool_via_the_allowlist_impact() {
        let _env = EnvGuard::set("explore,impact");

        assert_eq!(
            listed(),
            names(&["rustcodegraph_explore", "rustcodegraph_impact"])
        );
    }

    #[test]
    fn filters_list_tools_to_the_allowlisted_short_names() {
        let _env = EnvGuard::set("explore,search,node");

        assert_eq!(
            listed(),
            names(&[
                "rustcodegraph_explore",
                "rustcodegraph_node",
                "rustcodegraph_search",
            ])
        );
    }

    #[test]
    fn accepts_fully_qualified_rustcodegraph_names_and_ignores_whitespace() {
        let _env = EnvGuard::set(" rustcodegraph_explore , search ");

        assert_eq!(
            listed(),
            names(&["rustcodegraph_explore", "rustcodegraph_search"])
        );
    }

    #[test]
    fn treats_an_empty_whitespace_value_as_unset_default_surface() {
        let _env = EnvGuard::set("   ");
        let tools = listed();

        assert_eq!(tools.len(), 4);
        assert!(tools.contains(&"rustcodegraph_explore".to_string()));
    }

    #[test]
    fn rejects_a_disabled_tool_on_execute_defense_in_depth() {
        let _env = EnvGuard::set("node");
        let mut handler = ToolHandler::new(false);

        let res = handler.execute("rustcodegraph_explore", &Map::new());

        assert_eq!(res.is_error, Some(true));
        assert!(first_text(&res).contains("disabled via RUSTCODEGRAPH_MCP_TOOLS"));
    }

    #[test]
    fn lets_an_allowlisted_tool_past_the_guard() {
        let _env = EnvGuard::set("search");
        let mut handler = ToolHandler::new(false);

        // No RustCodeGraph attached, so it fails *after* the allowlist guard - the
        // "disabled" message must NOT appear, proving the guard passed it
        // through.
        let res = handler.execute("rustcodegraph_search", &args(&[("query", json!("x"))]));

        assert!(!first_text(&res).contains("disabled via RUSTCODEGRAPH_MCP_TOOLS"));
    }
}
