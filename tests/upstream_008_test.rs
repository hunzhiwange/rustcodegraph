use rustcodegraph::{db, types};
#[path = "../src/resolution/callback_synthesizer/common.rs"]
#[allow(dead_code)]
mod common;

#[test]
fn case_008_reuses_index() {
    let source = "中文😀\r\nsecond\nlast";
    let before = common::cache_builds();
    assert_eq!(common::slice_lines(source, 1, 2).unwrap(), "中文😀\nsecond");
    assert_eq!(common::slice_lines(source, 2, 3).unwrap(), "second\nlast");
    assert_eq!(common::cache_builds() - before, 1);
}

#[test]
fn case_008_boundaries() {
    for source in ["", "\n", "中😀\r\n尾\n", "a\rb"] {
        for start in 1..6 {
            for end in start..7 {
                let expected = source
                    .lines()
                    .skip(start - 1)
                    .take(end - start + 1)
                    .collect::<Vec<_>>()
                    .join("\n");
                assert_eq!(
                    common::slice_lines(source, start as u64, end as u64),
                    Some(expected)
                );
            }
        }
        assert_eq!(common::slice_lines(source, 0, 1), None);
    }
}

#[test]
fn case_008_same_length_update() {
    assert_eq!(common::slice_lines("a\nbc", 2, 2).unwrap(), "bc");
    assert_eq!(common::slice_lines("ab\nc", 2, 2).unwrap(), "c");
    assert_eq!(common::slice_lines("a\nbc", 2, 2).unwrap(), "bc");
}

#[test]
fn case_008_prefix_positions() {
    for source in ["", "中文😀\r\nlast\n", "ab\nc", "a\rb"] {
        for offset in (0..=source.len()).filter(|&i| source.is_char_boundary(i)) {
            assert_eq!(
                common::prefix_line_count(source, offset),
                source[..offset].lines().count() as u64
            );
        }
    }
}
