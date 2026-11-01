use rustcodegraph::mcp::index::resolve_daemon_root;
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn case_006_default_roots() {
    if std::env::var_os("CASE_006_HOME").is_none() {
        let home = std::env::temp_dir().join(format!("rcg-006-{}", std::process::id()));
        fs::create_dir_all(&home).unwrap();
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "case_006_default_roots", "--nocapture"])
            .env("CASE_006_HOME", &home)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .status()
            .unwrap();
        fs::remove_dir_all(home).unwrap();
        assert!(result.success());
        return;
    }
    let home = std::path::PathBuf::from(std::env::var_os("CASE_006_HOME").unwrap());
    let original = std::env::current_dir().unwrap();
    for name in ["project", ".ssh", ".config"] {
        let root = home.join(name);
        fs::create_dir_all(&root).unwrap();
        let mut graph = rustcodegraph::CodeGraph::init_sync(&root).unwrap();
        graph.close();
        for params in [
            serde_json::json!({}),
            serde_json::json!({"rootUri": format!("file://{}", root.display())}),
            serde_json::json!({"capabilities":{"roots":{}}}),
        ] {
            let roots = params.get("capabilities").is_some();
            let mut child = Command::new(env!("CARGO_BIN_EXE_rustcodegraph"))
                .args(["serve", "--mcp", "--no-watch", "--path"])
                .arg(&root)
                .env("RUSTCODEGRAPH_NO_DAEMON", "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            writeln!(
                input,
                "{}",
                serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":params})
            )
            .unwrap();
            writeln!(
                input,
                "{}",
                serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})
            )
            .unwrap();
            if roots {
                writeln!(input, "{}", serde_json::json!({"jsonrpc":"2.0","id":"roots/list:1","result":{"roots":[{"uri":format!("file://{}", root.display())}]}})).unwrap();
            }
            drop(input);
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            let lines: Vec<serde_json::Value> = String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            let list = lines.iter().find(|v| v["id"] == 2).unwrap();
            assert_eq!(
                list["result"]["tools"].as_array().unwrap().is_empty(),
                name != "project"
            );
        }
        if name == "project" {
            assert!(resolve_daemon_root(Some(&root)).is_some());
        } else {
            assert!(resolve_daemon_root(Some(&root)).is_none(), "{name}");
            std::env::set_current_dir(&root).unwrap();
            assert!(resolve_daemon_root(None).is_none());
            std::env::set_current_dir(&original).unwrap();
        }
    }
    std::env::set_current_dir(&home).unwrap();
}
