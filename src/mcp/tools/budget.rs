//! Explore output budgets scale with indexed file count.
//!
//! 这组阈值直接影响 agent 是否会回退 Read/Grep：repo 越大，可用调用数和
//! 单文件字符预算都不能倒退，尤其要保护大文件中的关键源码片段。

use std::env;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreOutputBudget {
    pub max_output_chars: usize,
    pub default_max_files: usize,
    pub max_chars_per_file: usize,
    pub gap_threshold: usize,
    pub max_symbols_in_file_header: usize,
    pub max_edges_per_relationship_kind: usize,
    pub include_relationships: bool,
    pub include_additional_files: bool,
    pub include_completeness_signal: bool,
    pub include_budget_note: bool,
    pub exclude_low_value_files: bool,
}

pub fn get_explore_budget(file_count: usize) -> usize {
    // 调用预算随 repo size 单调增加，上限 5，匹配 agent-eval 中的大仓探索策略。
    if file_count < 500 {
        1
    } else if file_count < 5_000 {
        2
    } else if file_count < 15_000 {
        3
    } else if file_count < 25_000 {
        4
    } else {
        5
    }
}

pub fn get_explore_output_budget(file_count: usize) -> ExploreOutputBudget {
    // `max_chars_per_file` 必须随 tier 单调不降；否则 god-file 仓库会只返回
    // 很小片段，迫使 agent 去 Read。
    if file_count < 150 {
        return ExploreOutputBudget {
            max_output_chars: 13_000,
            default_max_files: 4,
            max_chars_per_file: 3_800,
            gap_threshold: 7,
            max_symbols_in_file_header: 5,
            max_edges_per_relationship_kind: 4,
            include_relationships: false,
            include_additional_files: false,
            include_completeness_signal: false,
            include_budget_note: false,
            exclude_low_value_files: true,
        };
    }
    if file_count < 500 {
        return ExploreOutputBudget {
            max_output_chars: 18_000,
            default_max_files: 5,
            max_chars_per_file: 3_800,
            gap_threshold: 8,
            max_symbols_in_file_header: 6,
            max_edges_per_relationship_kind: 6,
            include_relationships: false,
            include_additional_files: false,
            include_completeness_signal: false,
            include_budget_note: false,
            exclude_low_value_files: true,
        };
    }
    if file_count < 5_000 {
        return ExploreOutputBudget {
            max_output_chars: 24_000,
            default_max_files: 8,
            max_chars_per_file: 6_500,
            gap_threshold: 12,
            max_symbols_in_file_header: 10,
            max_edges_per_relationship_kind: 10,
            include_relationships: true,
            include_additional_files: true,
            include_completeness_signal: true,
            include_budget_note: true,
            exclude_low_value_files: false,
        };
    }
    ExploreOutputBudget {
        max_output_chars: 24_000,
        default_max_files: 8,
        max_chars_per_file: 7_000,
        gap_threshold: 15,
        max_symbols_in_file_header: 15,
        max_edges_per_relationship_kind: 15,
        include_relationships: true,
        include_additional_files: true,
        include_completeness_signal: true,
        include_budget_note: true,
        exclude_low_value_files: false,
    }
}

pub fn explore_line_numbers_enabled() -> bool {
    env::var("RUSTCODEGRAPH_EXPLORE_LINENUMS").ok().as_deref() != Some("0")
}

pub fn adaptive_explore_enabled() -> bool {
    // 默认开启 adaptive explore；只有明确设为 0/false 才回到全量文件渲染。
    !matches!(
        env::var("RUSTCODEGRAPH_ADAPTIVE_EXPLORE").ok().as_deref(),
        Some("0") | Some("false")
    )
}

pub fn number_source_lines(slice: &str, first_line_number: usize) -> String {
    slice
        .split('\n')
        .enumerate()
        .map(|(i, line)| format!("{}\t{}", first_line_number + i, line))
        .collect::<Vec<_>>()
        .join("\n")
}
