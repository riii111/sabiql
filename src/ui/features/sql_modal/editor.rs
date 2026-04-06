use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::text_input::TextInputLike;
use crate::app::model::sql_editor::modal::SqlModalStatus;
use crate::ui::primitives::atoms::{CursorKind, cursor_style_for, highlight_sql_with_cursor_kind};
use crate::ui::theme::ThemePalette;

pub(super) fn render_editor(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    now: Instant,
    theme: &ThemePalette,
) {
    let content = state.sql_modal.editor.content();

    // Cursor and highlight are omitted to reinforce that the SQL is not editable here.
    if matches!(
        state.sql_modal.status(),
        SqlModalStatus::ConfirmingHigh { .. }
    ) {
        let lines: Vec<Line> = content
            .lines()
            .map(|line| {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(theme.text_muted),
                ))
            })
            .collect();
        let scroll_row = state.sql_modal.editor.scroll_row() as u16;
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((scroll_row, 0)),
            area,
        );
        return;
    }

    let is_normal = matches!(
        state.sql_modal.status(),
        SqlModalStatus::Normal | SqlModalStatus::Success | SqlModalStatus::Error
    );

    let (cursor_row, cursor_col) = state.sql_modal.editor.cursor_to_position();
    let current_line_style = Style::default().bg(theme.editor_current_line_bg);
    let cursor_kind = if is_normal {
        CursorKind::Block
    } else {
        CursorKind::Insert
    };
    let placeholder_cursor = if is_normal { " " } else { "\u{258f}" };

    let mut lines: Vec<Line> = if content.is_empty() {
        let placeholder = if is_normal {
            " Press Enter to edit..."
        } else {
            " Enter SQL query..."
        };
        vec![
            Line::from(vec![
                Span::styled(placeholder_cursor, cursor_style_for(cursor_kind, theme)),
                Span::styled(placeholder, Style::default().fg(theme.placeholder_text)),
            ])
            .style(current_line_style),
        ]
    } else {
        highlight_sql_with_cursor_kind(content, cursor_row, cursor_col, cursor_kind, theme)
            .into_iter()
            .enumerate()
            .map(|(row, line)| {
                if row == cursor_row {
                    line.style(current_line_style)
                } else {
                    line
                }
            })
            .collect()
    };

    if content.ends_with('\n') && cursor_row == content.lines().count() {
        lines.push(
            Line::from(vec![Span::styled(
                placeholder_cursor,
                cursor_style_for(cursor_kind, theme),
            )])
            .style(current_line_style),
        );
    }

    let flash_active = state.flash_timers.is_active(
        crate::app::model::shared::flash_timer::FlashId::SqlModal,
        now,
    );
    crate::ui::primitives::atoms::apply_yank_flash(&mut lines, flash_active, theme);

    let scroll_row = state.sql_modal.editor.scroll_row() as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll_row, 0))
            .style(Style::default()),
        area,
    );
}
