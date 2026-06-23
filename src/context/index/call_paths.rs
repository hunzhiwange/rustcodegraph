//! Markdown-only helpers for context guidance.
//!
//! 这些函数不改变 subgraph，只把已有边压缩成 agent 能快速消费的提示：
//! 低置信度时教它换精确 symbol，存在 calls 边时展示短调用链。

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::types::{Edge, EdgeKind, EdgeProvenance, Node, Subgraph};

use super::ContextBuilder;
use super::LOW_CONFIDENCE_MARKER;

impl<'a, 'db> ContextBuilder<'a, 'db> {
    pub(super) fn build_low_confidence_note(&self, entry_points: &[Node]) -> String {
        let mut dirs = Vec::new();
        let mut seen = HashSet::new();
        // 最多列几个候选目录即可；提示太长会稀释真正的补救动作。
        for node in entry_points {
            let dir = node
                .file_path
                .rfind('/')
                .filter(|idx| *idx > 0)
                .map(|idx| node.file_path[..idx].to_string())
                .unwrap_or_else(|| node.file_path.clone());
            if seen.insert(dir.clone()) {
                dirs.push(dir);
            }
            if dirs.len() >= 4 {
                break;
            }
        }

        let dir_line = if dirs.is_empty() {
            String::new()
        } else {
            format!(
                "\n- `rustcodegraph_files` a likely area: {}",
                dirs.iter()
                    .map(|dir| format!("`{dir}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        format!(
            "\n\n{LOW_CONFIDENCE_MARKER}\n\n\
This query matched mostly on common words, so the entry points above may be off-target — treat them as a starting point, not a complete answer. For a reliable result:\n\
- `rustcodegraph_explore` with the **exact symbol names** you are after (class / function / method names), or\n\
- `rustcodegraph_search <name>` for one specific symbol\
{dir_line}\n\nDo not assume the list above is comprehensive."
        )
    }

    pub(super) fn build_call_paths_section(&self, subgraph: &Subgraph) -> String {
        let mut adj = HashMap::<String, Vec<String>>::new();
        // 只展示 subgraph 内闭合的 calls 边，避免输出指向已被预算裁剪掉的节点。
        for edge in &subgraph.edges {
            if edge.kind != EdgeKind::Calls {
                continue;
            }
            if !subgraph.nodes.contains_key(&edge.source)
                || !subgraph.nodes.contains_key(&edge.target)
            {
                continue;
            }
            adj.entry(edge.source.clone())
                .or_default()
                .push(edge.target.clone());
        }
        if adj.is_empty() {
            return String::new();
        }

        let mut chains = Vec::<Vec<String>>::new();
        let mut budget = 2000usize;
        // roots 是 query 命中的入口；没有 roots 时退回稳定排序的任意 call 起点，
        // 这样测试和输出不会受 HashMap 顺序影响。
        let mut starts = if subgraph.roots.is_empty() {
            let mut keys = adj.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            keys
        } else {
            subgraph
                .roots
                .iter()
                .filter(|id| adj.contains_key(*id))
                .cloned()
                .collect::<Vec<_>>()
        };
        starts.truncate(5);

        for start in starts {
            let mut seen = HashSet::from([start.clone()]);
            let mut path = vec![start.clone()];
            collect_call_chains(&start, &mut path, &mut seen, &adj, &mut chains, &mut budget);
        }
        if chains.is_empty() {
            return String::new();
        }

        let root_set = subgraph.roots.iter().cloned().collect::<HashSet<_>>();
        // context 里可能有很多旁路调用。只保留连接至少两个 roots 的链，
        // 才更像“用户问的几个关键符号之间的路径”。
        let mut relevant = chains
            .into_iter()
            .filter(|chain| root_count(chain, &root_set) >= 2)
            .collect::<Vec<_>>();
        relevant.sort_by(|a, b| {
            root_count(b, &root_set)
                .cmp(&root_count(a, &root_set))
                .then(b.len().cmp(&a.len()))
        });

        let mut kept = Vec::<Vec<String>>::new();
        for chain in relevant {
            let key = chain.join(">");
            if kept
                .iter()
                .any(|kept_chain| kept_chain.join(">").contains(&key))
            {
                continue;
            }
            kept.push(chain);
            if kept.len() >= 3 {
                break;
            }
        }
        if kept.is_empty() {
            return String::new();
        }

        let synth_by_pair = synthesized_labels_by_pair(&subgraph.edges);
        let has_synth = kept.iter().any(|chain| {
            chain
                .windows(2)
                .any(|pair| synth_by_pair.contains_key(&format!("{}>{}", pair[0], pair[1])))
        });

        let mut lines = vec![
            String::new(),
            "## Call paths".to_string(),
            String::new(),
            "Execution flow among the key symbols (traced through the call graph):".to_string(),
            String::new(),
        ];
        for chain in &kept {
            lines.push(format!(
                "- {}",
                render_call_chain(chain, subgraph, &synth_by_pair)
            ));
        }
        lines.push(String::new());
        if has_synth {
            lines.push("_Hops marked `[callback/event …]` are dynamic dispatch bridged by rustcodegraph (with the registration site); the rest are direct calls. rustcodegraph_node any symbol for its body._".to_string());
        } else {
            lines.push(
                "_rustcodegraph_node any symbol above for its source + its own callers/callees._"
                    .to_string(),
            );
        }

        format!("\n{}\n", lines.join("\n"))
    }
}

fn collect_call_chains(
    id: &str,
    path: &mut Vec<String>,
    seen: &mut HashSet<String>,
    adj: &HashMap<String, Vec<String>>,
    chains: &mut Vec<Vec<String>>,
    budget: &mut usize,
) {
    if *budget == 0 {
        return;
    }
    *budget -= 1;

    let next = adj
        .get(id)
        .map(|targets| {
            targets
                .iter()
                .filter(|target| !seen.contains(*target))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // 这里刻意把链限制得很短：context 是入口提示，不是完整 trace 工具。
    if next.is_empty() || path.len() >= 6 {
        if path.len() >= 3 {
            chains.push(path.clone());
        }
        return;
    }

    for target in next {
        seen.insert(target.clone());
        path.push(target.clone());
        collect_call_chains(&target, path, seen, adj, chains, budget);
        path.pop();
        seen.remove(&target);
    }
}

fn root_count(chain: &[String], root_set: &HashSet<String>) -> usize {
    chain.iter().filter(|id| root_set.contains(*id)).count()
}

fn synthesized_labels_by_pair(edges: &[Edge]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for edge in edges {
        if edge.kind != EdgeKind::Calls || edge.provenance != Some(EdgeProvenance::Heuristic) {
            continue;
        }
        // synthesizedBy/registeredAt 是动态边的解释契约；缺少 metadata 时宁可不标注，
        // 也不要把普通 calls 边误报成 callback 或 framework hop。
        let Some(metadata) = edge.metadata.as_ref() else {
            continue;
        };
        let Some(synthesized_by) = metadata_string(metadata, "synthesizedBy") else {
            continue;
        };
        let at = metadata_string(metadata, "registeredAt")
            .map(|value| format!(" @{value}"))
            .unwrap_or_default();
        let label = match synthesized_by.as_str() {
            "callback" => {
                let via = metadata_string(metadata, "via")
                    .map(|value| format!("`{value}`"))
                    .unwrap_or_else(|| "registrar".to_string());
                format!("callback via {via}{at}")
            }
            "react-render" => format!("React re-render via setState{at}"),
            "jsx-render" => {
                let via = metadata_string(metadata, "via").unwrap_or_else(|| "child".to_string());
                format!("renders <{via}>")
            }
            "vue-handler" => {
                let event =
                    metadata_string(metadata, "event").unwrap_or_else(|| "event".to_string());
                format!("Vue @{event} handler")
            }
            _ => {
                let event = metadata_string(metadata, "event")
                    .map(|event| format!("`{event}`"))
                    .unwrap_or_default();
                format!("event {event}{at}")
            }
        };
        out.insert(format!("{}>{}", edge.source, edge.target), label);
    }
    out
}

fn metadata_string(metadata: &HashMap<String, Value>, key: &str) -> Option<String> {
    metadata.get(key).map(|value| match value {
        Value::String(value) => value.clone(),
        other => other.to_string(),
    })
}

fn render_call_chain(
    chain: &[String],
    subgraph: &Subgraph,
    synth_by_pair: &HashMap<String, String>,
) -> String {
    let Some(first) = chain.first() else {
        return String::new();
    };
    let mut rendered = node_name(subgraph, first);
    for pair in chain.windows(2) {
        let key = format!("{}>{}", pair[0], pair[1]);
        if let Some(synth) = synth_by_pair.get(&key) {
            rendered.push_str(&format!(" →[{synth}] {}", node_name(subgraph, &pair[1])));
        } else {
            rendered.push_str(&format!(" → {}", node_name(subgraph, &pair[1])));
        }
    }
    rendered
}

fn node_name(subgraph: &Subgraph, id: &str) -> String {
    subgraph
        .nodes
        .get(id)
        .map(|node| node.name.clone())
        .unwrap_or_else(|| id.to_string())
}
