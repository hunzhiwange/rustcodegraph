//! Read-only, per-file selection of flow evidence. Parse only when an owner
//! cannot fit, and share that one tree across all anchors in this response.
use super::super::budget::{explore_line_numbers_enabled, number_source_lines};
use super::flow::{AnchorKind, SourceAnchor};
use crate::{types::Node, web_tree_sitter::SyntaxNode};

#[derive(Debug)]
struct Span {
    start: usize,
    end: usize,
    reason: String,
}

#[derive(Debug, Default)]
struct Selection {
    spans: Vec<Span>,
    uncovered: Vec<u64>,
    parses: usize,
}

fn range(node: &SyntaxNode) -> (usize, usize) {
    (
        node.start_position.row,
        node.end_position.row + usize::from(node.end_position.column > 0),
    )
}

fn boundary(node: &SyntaxNode, line: usize, best: &mut Option<(usize, usize)>) {
    let (start, end) = range(node);
    if line < start || line >= end || node.has_error {
        return;
    }
    // Statement ancestors retain entire chains; JSX boundaries retain multiline
    // props without pulling in an arbitrarily large enclosing return expression.
    if matches!(
        node.node_type.as_str(),
        "expression_statement"
            | "return_statement"
            | "lexical_declaration"
            | "throw_statement"
            | "jsx_element"
            | "jsx_self_closing_element"
            | "jsx_opening_element"
    ) {
        if best.is_none_or(|(a, b)| end - start < b - a) {
            *best = Some((start, end));
        }
    }
    for child in &node.named_children {
        boundary(child, line, best);
    }
}

// Keep the complete condition/header of enclosing branches, independently of
// the (possibly distant) statement. AST coordinates ignore braces in literals.
fn branch_headers(node: &SyntaxNode, line: usize, out: &mut Vec<(usize, usize)>) {
    let (start, end) = range(node);
    if line < start || line >= end || node.has_error {
        return;
    }
    if matches!(
        node.node_type.as_str(),
        "if_statement" | "for_statement" | "for_in_statement" | "while_statement"
    ) {
        if let Some(body) = node
            .child_for_field_name("consequence")
            .or_else(|| node.child_for_field_name("body"))
        {
            if body.node_type == "statement_block" && body.start_position.row < line {
                out.push((start, body.start_position.row + 1));
            }
        }
    }
    for child in &node.named_children {
        branch_headers(child, line, out);
    }
}

fn signature(node: &SyntaxNode, line: usize) -> Option<(usize, usize)> {
    let (start, end) = range(node);
    if line < start || line >= end || node.has_error {
        return None;
    }
    if start == line
        && matches!(
            node.node_type.as_str(),
            "method_definition"
                | "function_declaration"
                | "public_field_definition"
                | "field_definition"
        )
    {
        fn body(node: &SyntaxNode) -> Option<&SyntaxNode> {
            if node.node_type == "statement_block" {
                return Some(node);
            }
            node.named_children.iter().find_map(body)
        }
        if let Some(body) = body(node) {
            return Some((start, body.start_position.row + 1));
        }
    }
    node.named_children
        .iter()
        .find_map(|child| signature(child, line))
}

fn render_span(span: &Span, lines: &[&str]) -> String {
    let body = lines[span.start..span.end].join("\n");
    format!(
        "... (source lines {}–{}; {})\n{}",
        span.start + 1,
        span.end,
        span.reason,
        if explore_line_numbers_enabled() {
            number_source_lines(&body, span.start + 1)
        } else {
            body
        }
    )
}

fn render_selection(selection: &Selection, lines: &[&str]) -> String {
    let mut parts: Vec<_> = selection
        .spans
        .iter()
        .map(|s| render_span(s, lines))
        .collect();
    if !selection.uncovered.is_empty() {
        parts.push(format!(
            "... (uncovered anchor lines: {}; source omitted)",
            selection
                .uncovered
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    parts.join("\n")
}

fn select(
    source: &str,
    file: &str,
    nodes: &[Node],
    anchors: &[SourceAnchor],
    cap: usize,
    compact: bool,
) -> Selection {
    let lines: Vec<_> = source.lines().collect();
    let anchors: Vec<_> = anchors.iter().filter(|a| a.file_path == file).collect();
    let mut selection = Selection::default();
    let mut tree = None;
    let mut parse_attempted = false;
    let reserve = 80 + anchors.len() * 22;
    for anchor in &anchors {
        let line = anchor.line.saturating_sub(1) as usize;
        if anchor.line == 0 || line >= lines.len() {
            selection.uncovered.push(anchor.line);
            continue;
        }
        let owner = nodes.iter().find(|n| {
            Some(&n.id) == anchor.owner_id.as_ref()
                && n.start_line <= anchor.line
                && anchor.line <= n.end_line
        });
        if anchor.owner_id.is_some() && owner.is_none() {
            selection.uncovered.push(anchor.line);
            continue;
        }
        let name = owner.map(|n| n.qualified_name.as_str()).unwrap_or("wiring");
        let full = owner.filter(|_| !compact).and_then(|n| {
            let span = Span {
                start: n.start_line.saturating_sub(1) as usize,
                end: (n.end_line as usize).min(lines.len()),
                reason: format!("complete {name}"),
            };
            let already_covered = selection
                .spans
                .iter()
                .any(|s| s.start <= span.start && span.end <= s.end);
            let needed = render_selection(&selection, &lines).len()
                + render_span(&span, &lines).len()
                + reserve;
            (span.start < span.end && (already_covered || needed <= cap)).then_some(span)
        });
        if full.is_none() && !parse_attempted {
            parse_attempted = true;
            let lang = crate::extraction::grammars::detect_language(file, None);
            if matches!(
                lang,
                crate::types::Language::TypeScript
                    | crate::types::Language::Tsx
                    | crate::types::Language::JavaScript
                    | crate::types::Language::Jsx
            ) {
                tree = crate::extraction::grammars::get_parser(lang).and_then(|mut parser| {
                    selection.parses += 1;
                    parser.parse(source, None)
                });
            }
        }
        let span = full.or_else(|| {
            let mut found = None;
            if let Some(tree) = &tree {
                boundary(&tree.root_node, line, &mut found);
            }
            // Label the complete signature as an excerpt, never a full method.
            if anchor.kind == AnchorKind::Definition || anchor.kind == AnchorKind::Mechanism {
                return tree
                    .as_ref()
                    .and_then(|tree| signature(&tree.root_node, line))
                    .map(|(start, end)| Span {
                        start,
                        end,
                        reason: format!("signature excerpt in {name}; body omitted"),
                    });
            }
            Some(match found {
                Some((start, end)) => Span {
                    start,
                    end,
                    reason: format!("statement excerpt in {name}"),
                },
                None => Span {
                    start: line,
                    end: line + 1,
                    reason: format!(
                        "line context in {name}; statement boundary unavailable; omitted context"
                    ),
                },
            })
        });
        let Some(span) = span else {
            selection.uncovered.push(anchor.line);
            continue;
        };
        let old = std::mem::take(&mut selection.spans);
        let mut pending = vec![span];
        if let Some(tree) = &tree {
            let mut headers = Vec::new();
            branch_headers(&tree.root_node, line, &mut headers);
            pending.extend(headers.into_iter().map(|(start, end)| Span {
                start,
                end,
                reason: format!("branch header in {name}; body excerpted"),
            }));
        }
        selection.spans = old
            .iter()
            .map(|s| Span {
                start: s.start,
                end: s.end,
                reason: s.reason.clone(),
            })
            .collect();
        for mut merged in pending {
            let mut next = Vec::new();
            for previous in std::mem::take(&mut selection.spans) {
                if previous.start <= merged.end && merged.start <= previous.end {
                    merged.start = merged.start.min(previous.start);
                    merged.end = merged.end.max(previous.end);
                    if previous.reason != merged.reason {
                        merged.reason = format!("{}; {}", previous.reason, merged.reason);
                    }
                } else {
                    next.push(previous);
                }
            }
            next.push(merged);
            next.sort_by_key(|s| s.start);
            selection.spans = next;
        }
        // Leave space for explicit evidence gaps, including subsequent anchors.
        if render_selection(&selection, &lines).len() + reserve > cap {
            selection.spans = old;
            selection.uncovered.push(anchor.line);
        }
    }
    selection.uncovered.sort_unstable();
    selection.uncovered.dedup();
    selection
}

pub(super) fn render(
    source: &str,
    file: &str,
    nodes: &[Node],
    anchors: &[SourceAnchor],
    cap: usize,
) -> String {
    let selection = select(source, file, nodes, anchors, cap, false);
    let lines: Vec<_> = source.lines().collect();
    let rendered = render_selection(&selection, &lines);
    if rendered.len() <= cap {
        rendered
    } else {
        let message = "... (uncovered anchors; source omitted)";
        if message.len() <= cap {
            message.to_string()
        } else {
            String::new()
        }
    }
}

/// Compact statement/signature evidence used to reserve the flow before extras.
pub(super) fn minimum(
    source: &str,
    file: &str,
    nodes: &[Node],
    anchors: &[SourceAnchor],
    cap: usize,
) -> String {
    let selection = select(source, file, nodes, anchors, cap, true);
    render_selection(&selection, &source.lines().collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn anchor(line: u64) -> SourceAnchor {
        SourceAnchor {
            file_path: "app.tsx".into(),
            line,
            owner_id: None,
            kind: AnchorKind::Wiring,
        }
    }

    #[test]
    fn adjacent_chained_statements_merge_and_parse_once() {
        let source = "function render() {\n  stream\n    .map(value => value + '🙂)')\n    .subscribe(consume);\n  register(\n    callback,\n  );\n}\n";
        let started = std::time::Instant::now();
        let selected = select(
            source,
            "app.tsx",
            &[],
            &[anchor(2), anchor(5), anchor(6)],
            2000,
            false,
        );

        assert_eq!(selected.parses, 1);
        assert_eq!(selected.spans.len(), 1);
        assert_eq!((selected.spans[0].start, selected.spans[0].end), (1, 7));
        assert!(selected.uncovered.is_empty());
        let output = render_selection(&selected, &source.lines().collect::<Vec<_>>());
        assert_eq!(output.matches(".subscribe(consume);").count(), 1);
        eprintln!(
            "snippet selection and rendering: parses={}, elapsed={:?}\n{output}",
            selected.parses,
            started.elapsed()
        );
    }

    #[test]
    fn keeps_multiline_branch_header_with_distant_statement() {
        let source = format!(
            "function render() {{\n if (\n   enabled && check('){{')\n ) {{\n{}  register(callback);\n }}\n}}\n",
            " // padding\n".repeat(100)
        );
        let selected = select(&source, "app.tsx", &[], &[anchor(105)], 1200, false);
        assert_eq!(selected.spans.len(), 2);
        assert_eq!((selected.spans[0].start, selected.spans[0].end), (1, 4));
        assert_eq!((selected.spans[1].start, selected.spans[1].end), (104, 105));
    }

    #[test]
    fn invalid_anchors_do_not_select_unrelated_source() {
        let source = "import { unrelated } from 'x';\n";
        let output = render(source, "app.tsx", &[], &[anchor(0), anchor(900)], 200);
        assert!(!output.contains("import"));
        assert!(output.contains("uncovered anchor lines: 0, 900"));
        assert!(output.len() <= 200);
    }

    #[test]
    fn missing_ast_labels_line_context_and_obeys_cap() {
        let mut a = anchor(2);
        a.file_path = "app.unknown".into();
        let source = "unrelated\n中文(\ncallback\n";
        let output = render(source, "app.unknown", &[], &[a.clone()], 500);
        assert!(output.contains("statement boundary unavailable"));
        assert!(!output.contains("unrelated"));
        for cap in [0, 1, 40, 100] {
            assert!(render(source, "app.unknown", &[], &[a.clone()], cap).len() <= cap);
        }
    }
}
