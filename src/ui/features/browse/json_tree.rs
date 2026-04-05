use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::app::model::browse::json_tree::{LineType, TreeLine, TreeValue};
use crate::ui::theme::ThemePalette;

const INDENT: &str = "  ";
const FOLD_EXPANDED: &str = "\u{25bc} "; // ▼
const FOLD_COLLAPSED: &str = "\u{25b6} "; // ▶

fn json_escaped(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
}

pub fn json_tree_line_spans(
    line: &TreeLine,
    is_selected: bool,
    theme: &ThemePalette,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    let indent = INDENT.repeat(line.depth);
    if !indent.is_empty() {
        spans.push(Span::styled(indent, Style::default().fg(theme.text_muted)));
    }

    match &line.line_type {
        LineType::ObjectOpen => {
            render_container_open(&mut spans, line, "{", "{...}", "keys", theme);
        }
        LineType::ObjectClose => {
            spans.push(Span::styled("}", Style::default().fg(theme.json_bracket)));
        }
        LineType::ArrayOpen => {
            render_container_open(&mut spans, line, "[", "[...]", "items", theme);
        }
        LineType::ArrayClose => {
            spans.push(Span::styled("]", Style::default().fg(theme.json_bracket)));
        }
        LineType::KeyValue => {
            push_key_spans(&mut spans, line.key.as_ref(), theme);
            push_value_span(&mut spans, &line.value, theme);
        }
        LineType::ArrayItem => {
            push_value_span(&mut spans, &line.value, theme);
        }
    }

    let line_style = if is_selected {
        Style::default().bg(theme.result_row_active_bg)
    } else {
        Style::default()
    };

    Line::from(spans).style(line_style)
}

fn render_container_open(
    spans: &mut Vec<Span<'static>>,
    line: &TreeLine,
    open_bracket: &'static str,
    collapsed_bracket: &'static str,
    count_label: &'static str,
    theme: &ThemePalette,
) {
    let fold = if line.collapsed {
        FOLD_COLLAPSED
    } else {
        FOLD_EXPANDED
    };
    spans.push(Span::styled(
        fold,
        Style::default().fg(theme.text_secondary),
    ));

    push_key_spans(spans, line.key.as_ref(), theme);

    let child_count = match &line.value {
        TreeValue::ObjectOpen { child_count } | TreeValue::ArrayOpen { child_count } => {
            *child_count
        }
        _ => 0,
    };

    if line.collapsed {
        spans.push(Span::styled(
            collapsed_bracket,
            Style::default().fg(theme.json_bracket),
        ));
    } else {
        spans.push(Span::styled(
            open_bracket,
            Style::default().fg(theme.json_bracket),
        ));
    }
    spans.push(Span::styled(
        format!(" [{child_count} {count_label}]"),
        Style::default().fg(theme.text_dim),
    ));
}

fn push_key_spans(spans: &mut Vec<Span<'static>>, key: Option<&String>, theme: &ThemePalette) {
    if let Some(key) = key {
        spans.push(Span::styled(
            json_escaped(key),
            Style::default().fg(theme.json_key),
        ));
        spans.push(Span::styled(
            ": ",
            Style::default().fg(theme.text_secondary),
        ));
    }
}

fn push_value_span(spans: &mut Vec<Span<'static>>, value: &TreeValue, theme: &ThemePalette) {
    match value {
        TreeValue::Null => {
            spans.push(Span::styled("null", Style::default().fg(theme.json_null)));
        }
        TreeValue::Bool(b) => {
            let s = if *b { "true" } else { "false" };
            spans.push(Span::styled(s, Style::default().fg(theme.json_bool)));
        }
        TreeValue::Number(n) => {
            spans.push(Span::styled(
                n.clone(),
                Style::default().fg(theme.json_number),
            ));
        }
        TreeValue::String(s) => {
            spans.push(Span::styled(
                json_escaped(s),
                Style::default().fg(theme.json_string),
            ));
        }
        TreeValue::ObjectOpen { .. } | TreeValue::ArrayOpen { .. } | TreeValue::Closing => {}
    }
}
