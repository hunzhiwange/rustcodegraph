use rustcodegraph::extraction::parse_worker::schedule_parses;
#[test]
fn case_017_stable_results() {
    assert_eq!(
        schedule_parses(&[3, 1, 2], 256, 64, |n| n * 2),
        vec![6, 2, 4]
    );
}
#[test]
fn case_017_empty_starts_no_work() {
    let result: Vec<()> = schedule_parses::<u8, _>(&[], 0, 0, |_| panic!("empty"));
    assert!(result.is_empty());
}
#[test]
fn case_017_memory_limits_concurrency() {
    let owner = std::thread::current().id();
    assert_eq!(
        schedule_parses(&[1, 2, 3], 1, 64, |_| {
            assert_eq!(std::thread::current().id(), owner);
            1
        }),
        vec![1; 3]
    );
}

#[test]
fn case_017_slow_file_does_not_pin_other_workers() {
    if std::thread::available_parallelism().unwrap().get() < 2 {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let rx = std::sync::Mutex::new(rx);
    let result = schedule_parses(&[0, 1, 2, 3], 128, 64, |n| {
        if *n == 0 {
            rx.lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
        } else if *n == 3 {
            tx.send(()).unwrap();
        }
        *n
    });
    assert_eq!(result, vec![0, 1, 2, 3]);
}

#[test]
fn case_017_native_results_match_serial() {
    use rustcodegraph::extraction::tree_sitter::extract_from_source;
    use rustcodegraph::types::Language;
    let jobs = [
        ("a.rs", "fn alpha() {}"),
        ("b.rs", "fn beta() { alpha(); }"),
    ];
    let extract =
        |job: &(&str, &str)| extract_from_source(job.0, job.1, Some(Language::Rust), None);
    let parallel = schedule_parses(&jobs, 128, 64, extract);
    for (job, result) in jobs.iter().zip(parallel) {
        let serial = extract(job);
        assert_eq!(
            serde_json::to_value(result.nodes).unwrap(),
            serde_json::to_value(serial.nodes).unwrap()
        );
        assert_eq!(result.errors.len(), serial.errors.len());
    }
}
