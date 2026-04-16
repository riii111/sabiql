use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::flash_timer::FlashId;
use crate::app::model::shared::text_input::TextInputLike;
use crate::app::model::sql_editor::modal::SqlModalStatus;
use crate::ui::primitives::atoms::{
    CursorKind, ModalTextSurface, build_modal_text_surface_lines, highlight_sql_spans,
    render_modal_text_surface,
};
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
                    Style::default().fg(theme.semantic.text.muted),
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
    let cursor_kind = if is_normal {
        CursorKind::Block
    } else {
        CursorKind::Insert
    };
    let surface = ModalTextSurface {
        content,
        cursor_row,
        cursor_col,
        scroll_row: state.sql_modal.editor.scroll_row(),
        cursor_kind,
        empty_placeholder: if is_normal {
            " Press i to edit..."
        } else {
            " Enter SQL query..."
        },
        base_style: Style::default(),
        current_line_style: Style::default().bg(theme.component.editor.current_line_bg),
    };
    let line_spans = highlight_sql_spans(content, theme);
    let mut lines = build_modal_text_surface_lines(surface, line_spans, theme);

    let flash_active = state.flash_timers.is_active(FlashId::SqlModal, now);
    crate::ui::primitives::atoms::apply_yank_flash(&mut lines, flash_active, theme);

    render_modal_text_surface(frame, area, surface, lines);
}
