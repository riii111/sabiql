use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::confirm_dialog::ConfirmIntent;
use crate::app::policy::json::json_diff::JsonDiffLine;
use crate::app::policy::write::write_guardrails::{RiskLevel, WriteOperation};
use crate::app::policy::write::write_update::escape_preview_value;
use crate::ui::primitives::atoms::highlight_sql;
use crate::ui::primitives::molecules::{render_modal, render_modal_with_border_color};
use crate::ui::primitives::utils::text_utils::wrapped_line_count;
use crate::ui::theme::ThemePalette;

pub struct ConfirmDialog;

pub struct ConfirmPreviewMetrics {
    pub viewport_height: Option<u16>,
    pub content_height: Option<u16>,
    pub scroll: u16,
}

impl ConfirmDialog {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        theme: &ThemePalette,
    ) -> ConfirmPreviewMetrics {
        if state.result_interaction.pending_write_preview().is_some() {
            Self::render_write_preview(frame, state, theme)
        } else {
            Self::render_plain(frame, state, theme)
        }
    }

    fn intent_border_color(intent: Option<&ConfirmIntent>, theme: &ThemePalette) -> Option<Color> {
        match intent {
            Some(ConfirmIntent::DisableReadOnly) => Some(theme.status_warning),
            Some(ConfirmIntent::DeleteConnection(_)) => Some(theme.status_error),
            _ => None,
        }
    }

    fn render_plain(
        frame: &mut Frame,
        state: &AppState,
        theme: &ThemePalette,
    ) -> ConfirmPreviewMetrics {
        let dialog = &state.confirm_dialog;
        let hint = " Enter: Confirm │ Esc: Cancel ";

        let full_area = frame.area();
        let max_modal_width = full_area.width.saturating_sub(2).max(20);
        let message_max_line = dialog
            .message()
            .lines()
            .map(|line| line.chars().count() as u16)
            .max()
            .unwrap_or(0);
        let hint_width = hint.chars().count() as u16;
        let title_width = dialog.title().chars().count() as u16;
        let content_width = message_max_line.max(hint_width).max(title_width);
        let preferred_width = content_width.saturating_add(6).max(40);
        let modal_width = preferred_width.min(max_modal_width);

        let message_width = modal_width.saturating_sub(4).max(1);
        let message_height = wrapped_line_count(dialog.message(), message_width);
        let max_modal_height = full_area.height.saturating_sub(2).max(6);
        let modal_height = (message_height + 2).clamp(6, max_modal_height);

        let title = format!(" {} ", dialog.title());
        let (_, modal_inner) =
            if let Some(color) = Self::intent_border_color(dialog.intent(), theme) {
                render_modal_with_border_color(
                    frame,
                    Constraint::Length(modal_width),
                    Constraint::Length(modal_height),
                    &title,
                    hint,
                    color,
                    theme,
                )
            } else {
                render_modal(
                    frame,
                    Constraint::Length(modal_width),
                    Constraint::Length(modal_height),
                    &title,
                    hint,
                    theme,
                )
            };

        let inner = modal_inner.inner(Margin::new(1, 0));
        let message_para = Paragraph::new(dialog.message().to_owned())
            .style(Style::default().fg(theme.text_primary))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });
        frame.render_widget(message_para, inner);
        ConfirmPreviewMetrics {
            viewport_height: None,
            content_height: None,
            scroll: 0,
        }
    }

    fn render_write_preview(
        frame: &mut Frame,
        state: &AppState,
        theme: &ThemePalette,
    ) -> ConfirmPreviewMetrics {
        let preview = state
            .result_interaction
            .pending_write_preview()
            .expect("write preview must be set");

        let border_color = theme.risk_color(preview.guardrail.risk_level);
        let blocked = preview.guardrail.blocked;
        let title = format!(" {} ", state.confirm_dialog.title());

        let mut content_lines: Vec<Line> = Vec::new();

        let risk_label = match preview.guardrail.risk_level {
            RiskLevel::Low => "✓ LOW RISK".to_string(),
            RiskLevel::Medium => "⚠ MEDIUM RISK: Multiple rows may be affected".to_string(),
            RiskLevel::High => format!(
                "⚠ HIGH RISK: {}",
                preview
                    .guardrail
                    .reason
                    .as_deref()
                    .unwrap_or("Execution is blocked")
            ),
        };
        content_lines.push(Line::from(Span::styled(
            risk_label,
            Style::default().fg(border_color),
        )));
        content_lines.push(Line::from(""));

        match preview.operation {
            WriteOperation::Update => {
                content_lines.push(Line::from(vec![Span::styled(
                    "Diff",
                    Style::default().fg(theme.text_secondary),
                )]));
                for (i, diff) in preview.diff.iter().enumerate() {
                    content_lines.push(Line::from(Span::styled(
                        format!("  {}:", diff.column),
                        Style::default().fg(theme.text_secondary),
                    )));
                    if let Some(json_lines) = &diff.json_diff {
                        Self::render_json_diff_lines(json_lines, &mut content_lines, theme);
                    } else {
                        let before = format!("\"{}\"", escape_preview_value(&diff.before));
                        let after = format!("\"{}\"", escape_preview_value(&diff.after));
                        content_lines.push(Line::from(Span::styled(
                            format!("    - {before}"),
                            Style::default().fg(theme.status_error),
                        )));
                        content_lines.push(Line::from(Span::styled(
                            format!("    + {after}"),
                            Style::default().fg(theme.status_success),
                        )));
                    }
                    if i + 1 < preview.diff.len() {
                        content_lines.push(Line::from(""));
                    }
                }
            }
            WriteOperation::Delete => {
                content_lines.push(Line::from(vec![Span::styled(
                    "Target",
                    Style::default().fg(theme.text_secondary),
                )]));
                for (key, value) in &preview.target_summary.key_values {
                    content_lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {key}: "),
                            Style::default().fg(theme.text_secondary),
                        ),
                        Span::styled(
                            format!("\"{}\"", escape_preview_value(value)),
                            Style::default().fg(theme.text_primary),
                        ),
                    ]));
                }
            }
        }

        content_lines.push(Line::from(""));

        content_lines.push(Line::from(vec![Span::styled(
            "SQL Preview",
            Style::default().fg(theme.text_secondary),
        )]));
        for sql_line in preview.sql.lines() {
            let indented = format!("  {sql_line}");
            content_lines.push(Self::highlight_sql_line(&indented, theme));
        }

        content_lines.push(Line::from(""));

        let full_area = frame.area();
        let terminal_cap = full_area.width.saturating_sub(2).max(20);
        let max_modal_width = (full_area.width * 70 / 100).max(44).min(terminal_cap);
        let hint_width_estimate: u16 = 50; // generous estimate for longest hint variant
        let title_width = title.chars().count() as u16;
        let content_max_width = content_lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.chars().count())
                    .sum::<usize>() as u16
            })
            .max()
            .unwrap_or(0);
        let content_width = content_max_width.max(hint_width_estimate).max(title_width);
        let preferred_width = content_width.saturating_add(6).max(44);
        let modal_width = preferred_width.min(max_modal_width);

        let inner_width = modal_width.saturating_sub(4).max(1);
        let content_text: String = content_lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let wrapped_height = wrapped_line_count(&content_text, inner_width);
        let max_modal_height = full_area.height.saturating_sub(2).max(6);
        let min_modal_height = 8.min(max_modal_height);
        // +4 = border top/bottom (2) + vertical padding (2)
        let modal_height = (wrapped_height + 4).clamp(min_modal_height, max_modal_height);

        // Determine scrollability and build hint string
        // inner_height = modal - border(2) - vertical padding(2)
        let inner_height = modal_height.saturating_sub(4);
        let scrollable = wrapped_height > inner_height;
        // Hint order: Actions → Navigation → Close/Cancel
        let hint: &str = match (scrollable, blocked) {
            (true, false) => " Enter: Confirm │ j/k/↑↓: Scroll │ Esc: Cancel ",
            (false, false) => " Enter: Confirm │ Esc: Cancel ",
            (true, true) => " j/k/↑↓: Scroll │ Esc: Cancel ",
            (false, true) => " Esc: Cancel ",
        };

        let (_, modal_inner) = render_modal_with_border_color(
            frame,
            Constraint::Length(modal_width),
            Constraint::Length(modal_height),
            &title,
            hint,
            border_color,
            theme,
        );

        let inner = modal_inner.inner(Margin::new(1, 1));

        let scroll = state
            .confirm_dialog
            .preview_scroll
            .min(wrapped_height.saturating_sub(inner.height));

        let para = Paragraph::new(content_lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));
        frame.render_widget(para, inner);
        ConfirmPreviewMetrics {
            viewport_height: Some(inner.height),
            content_height: Some(wrapped_height),
            scroll,
        }
    }

    fn render_json_diff_lines(
        lines: &[JsonDiffLine],
        output: &mut Vec<Line<'static>>,
        theme: &ThemePalette,
    ) {
        for line in lines {
            match line {
                JsonDiffLine::Context(s) => {
                    output.push(Line::from(Span::styled(
                        format!("    {s}"),
                        Style::default().fg(theme.text_dim),
                    )));
                }
                JsonDiffLine::Added(s) => {
                    output.push(Line::from(Span::styled(
                        format!("  + {s}"),
                        Style::default().fg(theme.status_success),
                    )));
                }
                JsonDiffLine::Removed(s) => {
                    output.push(Line::from(Span::styled(
                        format!("  - {s}"),
                        Style::default().fg(theme.status_error),
                    )));
                }
                JsonDiffLine::Ellipsis => {
                    output.push(Line::from(Span::styled(
                        "    ...".to_string(),
                        Style::default().fg(theme.text_dim),
                    )));
                }
            }
        }
    }

    fn highlight_sql_line(line: &str, theme: &ThemePalette) -> Line<'static> {
        highlight_sql(line, theme)
            .into_iter()
            .next()
            .unwrap_or_else(|| Line::from(""))
    }
}
