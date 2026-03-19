use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::sql_modal_context::SqlModalStatus;
use crate::app::state::AppState;
use crate::ui::theme::Theme;

use crate::ui::primitives::atoms::text_cursor_spans;
use crate::ui::primitives::molecules::{render_modal, render_modal_with_border_color};

mod completion;
mod cursor;
mod status;

pub struct SqlModal;

impl SqlModal {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let (area, inner) = match state.sql_modal.status() {
            SqlModalStatus::Confirming(decision) => {
                let title = format!(
                    " SQL \u{2500}\u{2500} \u{26a0} {} ",
                    decision.risk_level.as_str()
                );
                render_modal_with_border_color(
                    frame,
                    Constraint::Percentage(80),
                    Constraint::Percentage(60),
                    &title,
                    " Enter: Execute \u{2502} Esc: Back ",
                    Theme::risk_color(decision.risk_level),
                )
            }
            SqlModalStatus::ConfirmingHigh {
                decision,
                input,
                target_name,
            } => {
                let title = format!(
                    " SQL \u{2500}\u{2500} \u{26a0} {} ",
                    decision.risk_level.as_str()
                );
                let is_match = target_name
                    .as_ref()
                    .is_some_and(|name| input.content() == name);
                let footer = if is_match {
                    " Enter: Execute \u{2502} Esc: Back "
                } else {
                    " Esc: Back "
                };
                render_modal_with_border_color(
                    frame,
                    Constraint::Percentage(80),
                    Constraint::Percentage(60),
                    &title,
                    footer,
                    Theme::STATUS_ERROR,
                )
            }
            SqlModalStatus::Editing => render_modal(
                frame,
                Constraint::Percentage(80),
                Constraint::Percentage(60),
                " SQL Editor ",
                " \u{2325}Enter: Run \u{2502} ^L: Clear \u{2502} ^O: Hist \u{2502} Esc: Normal ",
            ),
            SqlModalStatus::Running => render_modal(
                frame,
                Constraint::Percentage(80),
                Constraint::Percentage(60),
                " SQL Editor ",
                " Running\u{2026} ",
            ),
            SqlModalStatus::Normal | SqlModalStatus::Success | SqlModalStatus::Error => {
                render_modal(
                    frame,
                    Constraint::Percentage(80),
                    Constraint::Percentage(60),
                    " SQL Editor ",
                    " \u{2325}Enter: Run \u{2502} y: Yank \u{2502} ^O: Hist \u{2502} Enter: Insert \u{2502} Esc: Close ",
                )
            }
        };

        let status_height = if matches!(
            state.sql_modal.status(),
            SqlModalStatus::ConfirmingHigh { .. }
        ) {
            3 // warning line + input prompt line + bottom margin
        } else {
            1
        };

        let [editor_area, status_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(status_height)]).areas(inner);

        Self::render_editor(frame, editor_area, state);
        status::render_status(frame, status_area, state);

        if matches!(state.sql_modal.status(), SqlModalStatus::Editing)
            && state.sql_modal.completion.visible
            && !state.sql_modal.completion.candidates.is_empty()
        {
            completion::render_completion_popup(frame, area, editor_area, state);
        }
    }

    fn render_editor(frame: &mut Frame, area: Rect, state: &AppState) {
        let content = &state.sql_modal.content;
        let now = Instant::now();
        let yank_flash_active = state
            .sql_modal
            .yank_flash_until
            .is_some_and(|until| now < until);

        // Cursor and highlight are omitted to reinforce that the SQL is not editable here.
        if matches!(
            state.sql_modal.status(),
            SqlModalStatus::Confirming(_) | SqlModalStatus::ConfirmingHigh { .. }
        ) {
            let lines: Vec<Line> = content
                .lines()
                .map(|line| {
                    Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(Theme::TEXT_MUTED),
                    ))
                })
                .collect();
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
            return;
        }

        let is_normal = matches!(
            state.sql_modal.status(),
            SqlModalStatus::Normal | SqlModalStatus::Success | SqlModalStatus::Error
        );

        let cursor_pos = state.sql_modal.cursor;
        let (cursor_row, cursor_col) = cursor::cursor_to_position(content, cursor_pos);
        let current_line_style = Style::default().bg(Theme::EDITOR_CURRENT_LINE_BG);

        let mut lines: Vec<Line> = if content.is_empty() {
            let placeholder = if is_normal {
                " Press Enter to edit..."
            } else {
                " Enter SQL query..."
            };
            if is_normal {
                vec![Line::from(Span::styled(
                    placeholder,
                    Style::default().fg(Theme::PLACEHOLDER_TEXT),
                ))]
            } else {
                vec![
                    Line::from(vec![
                        Span::styled("\u{2588}", Style::default().fg(Theme::CURSOR_FG)),
                        Span::styled(placeholder, Style::default().fg(Theme::PLACEHOLDER_TEXT)),
                    ])
                    .style(current_line_style),
                ]
            }
        } else if is_normal {
            content
                .lines()
                .enumerate()
                .map(|(row, line)| {
                    if row == cursor_row {
                        Line::from(line.to_string()).style(current_line_style)
                    } else {
                        Line::from(line.to_string())
                    }
                })
                .collect()
        } else {
            content
                .lines()
                .enumerate()
                .map(|(row, line)| {
                    if row == cursor_row {
                        Self::line_with_cursor(line, cursor_col).style(current_line_style)
                    } else {
                        Line::from(line.to_string())
                    }
                })
                .collect()
        };

        if !is_normal && content.ends_with('\n') && cursor_row == content.lines().count() {
            lines.push(
                Line::from(vec![Span::styled(
                    "\u{2588}",
                    Style::default().fg(Theme::CURSOR_FG),
                )])
                .style(current_line_style),
            );
        }

        if yank_flash_active {
            let flash_style = Style::default()
                .fg(Theme::YANK_FLASH_FG)
                .bg(Theme::YANK_FLASH_BG);
            for line in &mut lines {
                *line = std::mem::take(line).style(flash_style);
            }
        }

        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .style(Style::default()),
            area,
        );
    }

    fn line_with_cursor(line: &str, cursor_col: usize) -> Line<'static> {
        Line::from(text_cursor_spans(line, cursor_col, 0, usize::MAX))
    }
}
