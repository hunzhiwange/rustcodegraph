//! Archive integrity checks; these do not run or certify external experiments.
use serde_json::Value;
use std::path::Path;

#[test]
fn cancelled_matrix_inventories_preserve_scope_and_local_evidence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in [
        "terraform",
        "solidity",
        "erlang",
        "vbnet",
        "cobol",
        "cfml",
        "arkts",
        "cuda",
        "metal",
        "redux",
        "vue-store",
        "celery",
        "sidekiq",
        "mediatr",
        "spring-event",
        "laravel-event",
        "http-tier",
        "queue-tier",
        "socket-tier",
        "tauri",
    ] {
        let path = root.join(format!("docs/benchmarks/upstream-matrices/{name}.json"));
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["status"], "用户取消验收", "{name}");
        assert_eq!(value["external_experiments_executed"], false, "{name}");
        assert_eq!(value["runner_compatible"], false, "{name}");
        assert!(value["cases"].as_array().unwrap().is_empty(), "{name}");
        assert_eq!(
            value["source"]["commit"], "15483713763f344ff30fb7df92bcf091c7bd9758",
            "{name}"
        );
        let prerequisites = value["prerequisite_implementations"].as_array().unwrap();
        assert!(!prerequisites.is_empty(), "{name}");
        for prerequisite in prerequisites {
            for key in ["implementation", "local_checks"] {
                // Earlier inventories use either a single path or a path list.
                let paths: Vec<&str> = match &prerequisite[key] {
                    Value::String(path) => vec![path],
                    Value::Array(paths) => {
                        paths.iter().map(|path| path.as_str().unwrap()).collect()
                    }
                    _ => panic!("{name}: missing {key}"),
                };
                assert!(!paths.is_empty(), "{name}: empty {key}");
                for path in paths {
                    assert!(root.join(path).is_file(), "{name}: missing {path}");
                }
            }
        }
    }
}

#[test]
fn historical_nix_matrix_preserves_nine_distinct_pinned_inputs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("docs/benchmarks/upstream-matrices/nix.json");
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let cases = value.as_array().unwrap();
    assert_eq!(cases.len(), 9);
    let mut keys = std::collections::HashSet::new();
    for scale in ["small", "medium", "large"] {
        let selected: Vec<_> = cases.iter().filter(|case| case["scale"] == scale).collect();
        assert_eq!(selected.len(), 3, "{scale}");
        let mut questions = std::collections::HashSet::new();
        for case in selected {
            assert!(keys.insert(case["key"].as_str().unwrap()));
            assert!(questions.insert(case["question"].as_str().unwrap()));
            let commit = case["commit"].as_str().unwrap();
            assert_eq!(commit.len(), 40);
            assert!(commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert!(case["expected_path"].as_array().unwrap().len() >= 2);
        }
    }
}
