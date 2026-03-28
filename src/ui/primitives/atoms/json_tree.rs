use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::app::model::browse::json_tree::{LineType, TreeLine, TreeValue};
use crate::ui::theme::Theme;

const INDENT: &str = "  ";
const FOLD_EXPANDED: &str = "\u{25bc} "; // ▼
const FOLD_COLLAPSED: &str = "\u{25b6} "; // ▶

// JSON value type colors
const KEY_COLOR: ratatui::style::Color = Theme::SECTION_HEADER; // Cyan
const STRING_COLOR: ratatui::style::Color = Theme::STATUS_SUCCESS; // Green
const NUMBER_COLOR: ratatui::style::Color = Theme::TEXT_ACCENT; // Yellow
const BOOL_COLOR: ratatui::style::Color = Theme::STATUS_MEDIUM_RISK; // Orange
const NULL_COLOR: ratatui::style::Color = Theme::TEXT_MUTED; // DarkGray
const BRACKET_COLOR: ratatui::style::Color = Theme::TEXT_SECONDARY; // Gray
const COUNT_COLOR: ratatui::style::Color = Theme::TEXT_DIM;

pub fn json_tree_line_spans(line: &TreeLine, is_selected: bool) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Indentation
    let indent = INDENT.repeat(line.depth);
    if !indent.is_empty() {
        spans.push(Span::styled(indent, Style::default().fg(Theme::TEXT_MUTED)));
    }

    match &line.line_type {
        LineType::ObjectOpen => {
            // Fold indicator
            let fold = if line.collapsed {
                FOLD_COLLAPSED
            } else {
                FOLD_EXPANDED
            };
            spans.push(Span::styled(
                fold.to_string(),
                Style::default().fg(Theme::TEXT_SECONDARY),
            ));

            // Key prefix (if this object is a value of a key)
            if let Some(key) = &line.key {
                spans.push(Span::styled(
                    format!("\"{key}\""),
                    Style::default().fg(KEY_COLOR),
                ));
                spans.push(Span::styled(
                    ": ",
                    Style::default().fg(Theme::TEXT_SECONDARY),
                ));
            }

            if line.collapsed {
                if let TreeValue::ObjectOpen { child_count } = &line.value {
                    spans.push(Span::styled(
                        "{...}".to_string(),
                        Style::default().fg(BRACKET_COLOR),
                    ));
                    spans.push(Span::styled(
                        format!(" [{child_count} keys]"),
                        Style::default().fg(COUNT_COLOR),
                    ));
                }
            } else {
                spans.push(Span::styled("{", Style::default().fg(BRACKET_COLOR)));
                if let TreeValue::ObjectOpen { child_count } = &line.value {
                    spans.push(Span::styled(
                        format!(" [{child_count} keys]"),
                        Style::default().fg(COUNT_COLOR),
                    ));
                }
            }
        }

        LineType::ObjectClose => {
            spans.push(Span::styled("}", Style::default().fg(BRACKET_COLOR)));
        }

        LineType::ArrayOpen => {
            let fold = if line.collapsed {
                FOLD_COLLAPSED
            } else {
                FOLD_EXPANDED
            };
            spans.push(Span::styled(
                fold.to_string(),
                Style::default().fg(Theme::TEXT_SECONDARY),
            ));

            if let Some(key) = &line.key {
                spans.push(Span::styled(
                    format!("\"{key}\""),
                    Style::default().fg(KEY_COLOR),
                ));
                spans.push(Span::styled(
                    ": ",
                    Style::default().fg(Theme::TEXT_SECONDARY),
                ));
            }

            if line.collapsed {
                if let TreeValue::ArrayOpen { child_count } = &line.value {
                    spans.push(Span::styled(
                        "[...]".to_string(),
                        Style::default().fg(BRACKET_COLOR),
                    ));
                    spans.push(Span::styled(
                        format!(" [{child_count} items]"),
                        Style::default().fg(COUNT_COLOR),
                    ));
                }
            } else {
                spans.push(Span::styled("[", Style::default().fg(BRACKET_COLOR)));
                if let TreeValue::ArrayOpen { child_count } = &line.value {
                    spans.push(Span::styled(
                        format!(" [{child_count} items]"),
                        Style::default().fg(COUNT_COLOR),
                    ));
                }
            }
        }

        LineType::ArrayClose => {
            spans.push(Span::styled("]", Style::default().fg(BRACKET_COLOR)));
        }

        LineType::KeyValue => {
            if let Some(key) = &line.key {
                spans.push(Span::styled(
                    format!("\"{key}\""),
                    Style::default().fg(KEY_COLOR),
                ));
                spans.push(Span::styled(
                    ": ",
                    Style::default().fg(Theme::TEXT_SECONDARY),
                ));
            }
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

fn push_value_span(spans: &mut Vec<Span<'static>>, value: &TreeValue) {
    match value {
        TreeValue::Null => {
            spans.push(Span::styled("null", Style::default().fg(NULL_COLOR)));
        }
        TreeValue::Bool(b) => {
            spans.push(Span::styled(b.to_string(), Style::default().fg(BOOL_COLOR)));
        }
        TreeValue::Number(n) => {
            spans.push(Span::styled(n.clone(), Style::default().fg(NUMBER_COLOR)));
        }
        TreeValue::String(s) => {
            spans.push(Span::styled(
                format!("\"{s}\""),
                Style::default().fg(STRING_COLOR),
            ));
        }
        TreeValue::ObjectOpen { .. } | TreeValue::ArrayOpen { .. } | TreeValue::Closing => {}
    }
}
