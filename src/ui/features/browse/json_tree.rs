use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::app::model::browse::json_tree::{LineType, TreeLine, TreeValue};
use crate::ui::theme::Theme;

const INDENT: &str = "  ";
const FOLD_EXPANDED: &str = "\u{25bc} "; // ▼
const FOLD_COLLAPSED: &str = "\u{25b6} "; // ▶

const KEY_COLOR: ratatui::style::Color = Theme::JSON_KEY;
const STRING_COLOR: ratatui::style::Color = Theme::JSON_STRING;
const NUMBER_COLOR: ratatui::style::Color = Theme::JSON_NUMBER;
const BOOL_COLOR: ratatui::style::Color = Theme::JSON_BOOL;
const NULL_COLOR: ratatui::style::Color = Theme::JSON_NULL;
const BRACKET_COLOR: ratatui::style::Color = Theme::JSON_BRACKET;
const COUNT_COLOR: ratatui::style::Color = Theme::TEXT_DIM;

fn json_escaped(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
}

pub fn json_tree_line_spans(line: &TreeLine, is_selected: bool) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    let indent = INDENT.repeat(line.depth);
    if !indent.is_empty() {
        spans.push(Span::styled(indent, Style::default().fg(Theme::TEXT_MUTED)));
    }

    match &line.line_type {
        LineType::ObjectOpen => {
            render_container_open(&mut spans, line, "{", "{...}", "keys");
        }
        LineType::ObjectClose => {
            spans.push(Span::styled("}", Style::default().fg(BRACKET_COLOR)));
        }
        LineType::ArrayOpen => {
            render_container_open(&mut spans, line, "[", "[...]", "items");
        }
        LineType::ArrayClose => {
            spans.push(Span::styled("]", Style::default().fg(BRACKET_COLOR)));
        }
        LineType::KeyValue => {
            push_key_spans(&mut spans, line.key.as_ref());
            push_value_span(&mut spans, &line.value);
        }
        LineType::ArrayItem => {
            push_value_span(&mut spans, &line.value);
        }
    }

    let line_style = if is_selected {
        Style::default().bg(Theme::RESULT_ROW_ACTIVE_BG)
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
) {
    let fold = if line.collapsed {
        FOLD_COLLAPSED
    } else {
        FOLD_EXPANDED
    };
    spans.push(Span::styled(
        fold,
        Style::default().fg(Theme::TEXT_SECONDARY),
    ));

    push_key_spans(spans, line.key.as_ref());

    let child_count = match &line.value {
        TreeValue::ObjectOpen { child_count } | TreeValue::ArrayOpen { child_count } => {
            *child_count
        }
        _ => 0,
    };

    if line.collapsed {
        spans.push(Span::styled(
            collapsed_bracket,
            Style::default().fg(BRACKET_COLOR),
        ));
    } else {
        spans.push(Span::styled(
            open_bracket,
            Style::default().fg(BRACKET_COLOR),
        ));
    }
    spans.push(Span::styled(
        format!(" [{child_count} {count_label}]"),
        Style::default().fg(COUNT_COLOR),
    ));
}

fn push_key_spans(spans: &mut Vec<Span<'static>>, key: Option<&String>) {
    if let Some(key) = key {
        spans.push(Span::styled(
            json_escaped(key),
            Style::default().fg(KEY_COLOR),
        ));
        spans.push(Span::styled(
            ": ",
            Style::default().fg(Theme::TEXT_SECONDARY),
        ));
    }
}

fn push_value_span(spans: &mut Vec<Span<'static>>, value: &TreeValue) {
    match value {
        TreeValue::Null => {
            spans.push(Span::styled("null", Style::default().fg(NULL_COLOR)));
        }
        TreeValue::Bool(b) => {
            let s = if *b { "true" } else { "false" };
            spans.push(Span::styled(s, Style::default().fg(BOOL_COLOR)));
        }
        TreeValue::Number(n) => {
            spans.push(Span::styled(n.clone(), Style::default().fg(NUMBER_COLOR)));
        }
        TreeValue::String(s) => {
            spans.push(Span::styled(
                json_escaped(s),
                Style::default().fg(STRING_COLOR),
            ));
        }
        TreeValue::ObjectOpen { .. } | TreeValue::ArrayOpen { .. } | TreeValue::Closing => {}
    }
}
