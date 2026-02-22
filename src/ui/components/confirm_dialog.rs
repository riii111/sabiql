use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::molecules::{render_modal, render_modal_with_border_color};
use crate::app::state::AppState;
use crate::app::write_guardrails::WritePreview;
use crate::app::write_update::escape_preview_value;
use crate::ui::theme::Theme;

pub struct ConfirmDialog;

impl ConfirmDialog {
    fn wrapped_line_count(text: &str, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }

        text.lines()
            .map(|line| {
                let chars = line.chars().count() as u16;
                chars.max(1).div_ceil(width)
            })
            .sum()
    }

    pub fn render(frame: &mut Frame, state: &AppState) {
        if let Some(preview) = &state.pending_write_preview {
            Self::render_write_preview(frame, state, preview);
        } else {
            Self::render_plain(frame, state);
        }
    }

    fn render_plain(frame: &mut Frame, state: &AppState) {
        let dialog = &state.confirm_dialog;
        let hint = " Enter/Y: Confirm │ Esc/N: Cancel ";

        let full_area = frame.area();
        let max_modal_width = full_area.width.saturating_sub(2).max(20);
        let message_max_line = dialog
            .message
            .lines()
            .map(|line| line.chars().count() as u16)
            .max()
            .unwrap_or(0);
        let hint_width = hint.chars().count() as u16;
        let title_width = dialog.title.chars().count() as u16;
        let content_width = message_max_line.max(hint_width).max(title_width);
        let preferred_width = content_width.saturating_add(6).max(40);
        let modal_width = preferred_width.min(max_modal_width);

        let message_width = modal_width.saturating_sub(4).max(1);
        let message_height = Self::wrapped_line_count(&dialog.message, message_width);
        let max_modal_height = full_area.height.saturating_sub(2).max(6);
        let modal_height = (message_height + 2).clamp(6, max_modal_height);

        let title = format!(" {} ", dialog.title);
        let (_, modal_inner) = render_modal(
            frame,
            Constraint::Length(modal_width),
            Constraint::Length(modal_height),
            &title,
            hint,
        );

        let inner = modal_inner.inner(Margin::new(1, 0));
        let message_para = Paragraph::new(dialog.message.clone())
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });
        frame.render_widget(message_para, inner);
    }

    fn render_write_preview(frame: &mut Frame, state: &AppState, preview: &WritePreview) {
        let hint = " Enter/Y: Confirm │ Esc/N: Cancel ";
        let title = format!(" {} ", state.confirm_dialog.title);

        // Build styled lines
        let mut content_lines: Vec<Line> = Vec::new();

        // Section: Diff
        content_lines.push(Line::from(vec![Span::styled(
            "Diff",
            Style::default().fg(Theme::TEXT_SECONDARY),
        )]));
        for diff in &preview.diff {
            let before = format!("\"{}\"", escape_preview_value(&diff.before));
            let after = format!("\"{}\"", escape_preview_value(&diff.after));
            content_lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}: ", diff.column),
                    Style::default().fg(Theme::TEXT_SECONDARY),
                ),
                Span::styled(before, Style::default().fg(Theme::TEXT_PRIMARY)),
                Span::styled("  →  ", Style::default().fg(Theme::TEXT_SECONDARY)),
                Span::styled(after, Style::default().fg(Theme::TEXT_PRIMARY)),
            ]));
        }

        content_lines.push(Line::from(""));

        // Section: SQL Preview
        content_lines.push(Line::from(vec![Span::styled(
            "SQL Preview",
            Style::default().fg(Theme::TEXT_SECONDARY),
        )]));
        for sql_line in preview.sql.lines() {
            let indented = format!("  {}", sql_line);
            content_lines.push(Self::highlight_sql_line(&indented));
        }

        // Trailing blank line for spacing before footer hint
        content_lines.push(Line::from(""));

        // Calculate modal dimensions from content
        let full_area = frame.area();
        let max_modal_width = full_area.width.saturating_sub(2).max(20);
        let hint_width = hint.chars().count() as u16;
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
        let content_width = content_max_width.max(hint_width).max(title_width);
        let preferred_width = content_width.saturating_add(6).max(44);
        let modal_width = preferred_width.min(max_modal_width);

        let content_height = content_lines.len() as u16;
        let max_modal_height = full_area.height.saturating_sub(2).max(6);
        let modal_height = (content_height + 2).clamp(8, max_modal_height);

        let (_, modal_inner) = render_modal_with_border_color(
            frame,
            Constraint::Length(modal_width),
            Constraint::Length(modal_height),
            &title,
            hint,
            Theme::STATUS_WARNING,
        );

        let inner = modal_inner.inner(Margin::new(1, 0));
        let para = Paragraph::new(content_lines).alignment(Alignment::Left);
        frame.render_widget(para, inner);
    }

    fn highlight_sql_line(line: &str) -> Line<'static> {
        const SQL_KEYWORDS: &[&str] = &["UPDATE", "SET", "WHERE", "AND", "OR", "NULL"];

        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];

        let mut spans: Vec<Span<'static>> = Vec::new();
        if !indent.is_empty() {
            spans.push(Span::raw(indent.to_string()));
        }

        // Check if line starts with a SQL keyword
        let keyword_hit = SQL_KEYWORDS.iter().find(|&&kw| {
            trimmed.starts_with(kw)
                && trimmed[kw.len()..].starts_with(|c: char| c.is_whitespace() || c == ';')
        });

        if let Some(&kw) = keyword_hit {
            spans.push(Span::styled(
                kw.to_string(),
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                trimmed[kw.len()..].to_string(),
                Style::default().fg(Color::White),
            ));
        } else {
            spans.push(Span::styled(
                trimmed.to_string(),
                Style::default().fg(Color::White),
            ));
        }

        Line::from(spans)
    }
}
