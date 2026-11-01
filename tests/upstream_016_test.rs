use rustcodegraph::extraction::grammars::{get_parser, reset_parser};
use rustcodegraph::types::Language;

#[test]
fn case_016_concurrent_native_parsers() {
    let threads: Vec<_> = (0..8)
        .map(|i| {
            std::thread::spawn(move || {
                let mut parser = get_parser(Language::Rust).unwrap();
                for n in 0..20 {
                    let source = format!("fn worker_{i}_{n}() {{}}");
                    let tree = parser.parse(&source, None).unwrap();
                    assert_eq!(tree.root_node.text, source);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
fn case_016_parser_configuration_is_isolated() {
    let mut first = get_parser(Language::Rust).unwrap();
    first.set_timeout_micros(123);
    first.set_language(None);
    let mut second = get_parser(Language::Rust).unwrap();
    assert_eq!(second.get_timeout_micros(), 0);
    assert!(first.parse("fn first() {}", None).is_none());
    assert!(second.parse("fn second() {}", None).is_some());
}

#[test]
fn case_016_reset_and_invalid_language_do_not_poison_existing_parser() {
    let mut existing = get_parser(Language::Rust).unwrap();
    assert!(get_parser(Language::Unknown).is_none());
    reset_parser(Language::Rust);
    assert!(existing.parse("fn retained() {}", None).is_some());
    assert!(
        get_parser(Language::Rust)
            .unwrap()
            .parse("fn fresh() {}", None)
            .is_some()
    );
}
