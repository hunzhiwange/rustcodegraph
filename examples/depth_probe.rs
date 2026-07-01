// Throwaway: measure extract_from_source cost vs nesting depth.
use rustcodegraph::extraction::index::extract_from_source;
use std::io::Write;
use std::time::Instant;

fn rss_mb() -> u64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or(0)
        / 1024
}

fn nested_rust(depth: usize) -> String {
    let mut s = String::from("fn deep() -> i32 {\n");
    for i in 0..depth {
        s.push_str(&format!("{}if x{} {{ ", "    ".repeat(i + 1), i));
    }
    s.push('1');
    for _ in 0..depth {
        s.push_str(" } else { 0 }");
    }
    s.push_str("\n}\n");
    s
}

fn main() {
    for depth in [5usize, 10, 15, 20, 25, 30, 35] {
        let src = nested_rust(depth);
        let before = rss_mb();
        let t = Instant::now();
        let r = extract_from_source("deep.rs", &src, None, None);
        let dt = t.elapsed().as_millis();
        let after = rss_mb();
        println!(
            "depth={depth:>3} src_len={:>6} nodes={:>4} time={dt:>7}ms rss {before}->{after}MB (+{}MB)",
            src.len(),
            r.nodes.len(),
            after.saturating_sub(before)
        );
        std::io::stdout().flush().unwrap();
    }
}
