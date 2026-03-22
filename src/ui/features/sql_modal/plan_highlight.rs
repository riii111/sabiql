use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::ui::theme::Theme;

const NODE_TYPES: &[&str] = &[
    "Aggregate",
    "Append",
    "Bitmap Heap Scan",
    "Bitmap Index Scan",
    "CTE Scan",
    "Delete",
    "Foreign Scan",
    "Function Scan",
    "Gather Merge",
    "Gather",
    "Group",
    "Hash Anti Join",
    "Hash Join",
    "Hash Semi Join",
    "Hash",
    "Incremental Sort",
    "Index Only Scan",
    "Index Scan",
    "Insert",
    "Limit",
    "LockRows",
    "Materialize",
    "Memoize",
    "Merge Append",
    "Merge Join",
    "Nested Loop",
    "Parallel Bitmap Heap Scan",
    "Parallel Index Only Scan",
    "Parallel Index Scan",
    "Parallel Seq Scan",
    "ProjectSet",
    "Result",
    "Seq Scan",
    "SetOp",
    "Sort",
    "Subquery Scan",
    "Unique",
    "Update",
    "Values Scan",
    "WindowAgg",
];

pub fn highlight_plan_line(raw: &str) -> Line<'static> {
    let trimmed = raw.trim_start();
    // ASCII-only: PostgreSQL EXPLAIN output uses space indentation, never multibyte
    let leading_spaces = raw.len() - trimmed.len();
    let content = trimmed.trim_start_matches("->").trim_start();

    let mut spans: Vec<Span<'static>> = Vec::new();

    if leading_spaces > 0 {
        spans.push(Span::raw(" ".repeat(leading_spaces)));
    }

    if let Some(cost_paren) = content.find("(cost=") {
        let before_cost = &content[..cost_paren];
        let cost_part = &content[cost_paren..];

        let node_style = Style::default()
            .fg(Theme::SECTION_HEADER)
            .add_modifier(Modifier::BOLD);
        let cost_style = Style::default().fg(Theme::TEXT_DIM);

        if let Some(node_name) = find_node_type(before_cost) {
            let after_node = &before_cost[node_name.len()..];
            spans.push(Span::styled(node_name.to_string(), node_style));
            spans.push(Span::raw(after_node.to_string()));
        } else {
            spans.push(Span::raw(before_cost.to_string()));
        }

        spans.push(Span::styled(cost_part.to_string(), cost_style));
    } else if let Some(node_name) = find_node_type(content) {
        let node_style = Style::default()
            .fg(Theme::SECTION_HEADER)
            .add_modifier(Modifier::BOLD);
        let after_node = &content[node_name.len()..];
        spans.push(Span::styled(node_name.to_string(), node_style));
        spans.push(Span::raw(after_node.to_string()));
    } else {
        spans.push(Span::raw(content.to_string()));
    }

    Line::from(spans)
}

pub(super) fn highlight_truncated(raw: &str, width: usize) -> Vec<Span<'static>> {
    let truncated = super::compare::pad_or_truncate(raw, width);
    let trimmed = truncated.trim_start();
    let content = trimmed.trim_start_matches("->").trim_start();

    let node_style = Style::default()
        .fg(Theme::SECTION_HEADER)
        .add_modifier(Modifier::BOLD);
    let cost_style = Style::default().fg(Theme::TEXT_DIM);

    let prefix_len = truncated.len() - content.len();
    let prefix = &truncated[..prefix_len];

    if let Some(cost_idx) = content.find("(cost=") {
        let before_cost = &content[..cost_idx];
        let cost_part = &content[cost_idx..];

        if let Some(node_name) = find_node_type(before_cost) {
            let after_node = &before_cost[node_name.len()..];
            vec![
                Span::raw(prefix.to_string()),
                Span::styled(node_name.to_string(), node_style),
                Span::raw(after_node.to_string()),
                Span::styled(cost_part.to_string(), cost_style),
            ]
        } else {
            vec![
                Span::raw(prefix.to_string()),
                Span::raw(before_cost.to_string()),
                Span::styled(cost_part.to_string(), cost_style),
            ]
        }
    } else if let Some(node_name) = find_node_type(content) {
        let after_node = &content[node_name.len()..];
        vec![
            Span::raw(prefix.to_string()),
            Span::styled(node_name.to_string(), node_style),
            Span::raw(after_node.to_string()),
        ]
    } else {
        vec![Span::raw(truncated)]
    }
}

fn find_node_type(text: &str) -> Option<&'static str> {
    NODE_TYPES.iter().find(|&&nt| text.starts_with(nt)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn seq_scan_line_returns_highlighted_node_and_cost() {
        let line = highlight_plan_line("Seq Scan on users  (cost=0.00..10.20 rows=10 width=3273)");
        let text = spans_text(&line);
        assert!(text.contains("Seq Scan"));
        assert!(text.contains("(cost="));
    }

    #[test]
    fn nested_node_returns_indented_text_without_guide() {
        let line = highlight_plan_line(
            "    ->  Index Scan using idx on users  (cost=0.28..8.30 rows=1 width=64)",
        );

        let text = spans_text(&line);

        assert!(text.starts_with("    "));
        assert!(text.contains("Index Scan"));
    }

    #[test]
    fn filter_line_returns_raw_text() {
        let line = highlight_plan_line("        Filter: (id > 10)");
        let text = spans_text(&line);
        assert!(text.contains("Filter:"));
    }

    #[test]
    fn empty_input_returns_non_empty_spans() {
        let line = highlight_plan_line("");
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn deeply_nested_line_returns_space_indentation_without_guides() {
        let line =
            highlight_plan_line("            ->  Hash  (cost=100.00..100.00 rows=1000 width=32)");

        let text = spans_text(&line);

        assert!(text.starts_with("            "));
    }
}
