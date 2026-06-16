use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::model::browse::cell_detail::CellDetailMode;
use crate::app::model::shared::flash_timer::FlashId;
use crate::app::model::shared::text_input::TextInputLike;
use crate::app::update::helpers::casefold_match_len;
use crate::features::browse::detail_view::{render_detail_search, search_match_status};
use crate::primitives::atoms::{
    CursorKind, ModalTextSurface, apply_yank_flash, build_modal_text_surface_lines,
    render_modal_text_surface,
};
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::theme::ThemePalette;

pub struct CellDetailRenderMetrics {
    pub visible_rows: usize,
    pub viewport_width: usize,
}

pub struct CellDetail;

impl CellDetail {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        now: std::time::Instant,
        theme: &ThemePalette,
    ) -> Option<CellDetailRenderMetrics> {
        if !state.cell_detail.is_active() {
            return None;
        }

        let is_editing = state.cell_detail.mode() == CellDetailMode::Editing;
        let title = if is_editing {
            format!(
                " Cell Detail Edit \u{2500}\u{2500} {}",
                state.cell_detail.column_name()
            )
        } else {
            format!(
                " Cell Detail \u{2500}\u{2500} {}",
                state.cell_detail.column_name()
            )
        };
        let hints = if is_editing {
            vec![("Esc", "Normal")]
        } else {
            vec![
                ("y", "Copy"),
                ("/", "Search"),
                ("i", "Edit"),
                ("Esc", "Close"),
            ]
        };
        let (_area, inner) = render_modal(
            frame,
            Constraint::Percentage(80),
            Constraint::Percentage(70),
            &title,
            FooterHintBar::new(hints),
            theme,
        );

        let (content_area, status_area, search_area) = if state.cell_detail.search().is_active() {
            let [content_area, status_area, search_area] = Layout::vertical([
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(inner);
            (content_area, status_area, Some(search_area))
        } else {
            let [content_area, status_area] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
            (content_area, status_area, None)
        };

        Self::render_editor_content(frame, content_area, state, is_editing, now, theme);
        Self::render_status(frame, status_area, state, theme);
        if let Some(search_area) = search_area {
            Self::render_search(frame, search_area, state, theme);
        }

        Some(CellDetailRenderMetrics {
            visible_rows: content_area.height as usize,
            viewport_width: content_area.width as usize,
        })
    }

    fn render_editor_content(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        is_editing: bool,
        now: std::time::Instant,
        theme: &ThemePalette,
    ) {
        let editor = state.cell_detail.editor();
        let content = editor.content();
        let (cursor_row, cursor_col) = editor.cursor_to_position();
        let cursor_kind = if is_editing {
            CursorKind::Insert
        } else {
            CursorKind::Block
        };
        let surface = ModalTextSurface {
            content,
            cursor_row,
            cursor_col,
            scroll_row: editor.scroll_row(),
            cursor_kind,
            empty_placeholder: if is_editing {
                " Enter value..."
            } else {
                " Press i to edit..."
            },
            base_style: Style::default().fg(theme.semantic.text.primary),
            current_line_style: Style::default().bg(theme.component.editor.current_line_bg),
        };
        let line_spans = highlighted_line_spans(state, theme);
        let mut lines = build_modal_text_surface_lines(surface, line_spans, theme);

        let flash_active = state.flash_timers.is_active(FlashId::CellDetail, now);
        apply_yank_flash(&mut lines, flash_active, theme);

        render_modal_text_surface(frame, area, surface, lines);
    }

    fn render_status(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) {
        let search = state.cell_detail.search();
        let status = if search.is_active() {
            search_match_status(search)
        } else {
            format!(
                "{} chars",
                state.cell_detail.current_content().chars().count()
            )
        };

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                status,
                Style::default().fg(theme.semantic.text.muted),
            ))),
            area,
        );
    }

    fn render_search(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) {
        render_detail_search(frame, area, state.cell_detail.search(), theme);
    }
}

fn highlighted_line_spans(state: &AppState, theme: &ThemePalette) -> Vec<Vec<Span<'static>>> {
    let content = state.cell_detail.editor().content();
    let current_match = state
        .cell_detail
        .search()
        .matches()
        .get(state.cell_detail.search().current_match())
        .copied();
    let query = state.cell_detail.search().input().content();
    let mut char_offset = 0;

    content
        .lines()
        .map(|line| {
            let line_len = line.chars().count();
            let line_start = char_offset;
            let line_end = line_start + line_len;
            char_offset = line_end + 1;

            match current_match {
                Some(pos) if !query.is_empty() && pos >= line_start && pos < line_end => {
                    highlighted_spans(line, pos - line_start, query, theme)
                }
                _ => vec![Span::raw(line.to_owned())],
            }
        })
        .collect()
}

fn highlighted_spans(
    line: &str,
    match_start: usize,
    query: &str,
    theme: &ThemePalette,
) -> Vec<Span<'static>> {
    let match_len = casefold_match_len(line, match_start, query);
    let mut before = String::new();
    let mut matched = String::new();
    let mut after = String::new();

    for (idx, ch) in line.chars().enumerate() {
        if idx < match_start {
            before.push(ch);
        } else if idx < match_start + match_len {
            matched.push(ch);
        } else {
            after.push(ch);
        }
    }

    vec![
        Span::raw(before),
        Span::styled(
            matched,
            Style::default()
                .fg(theme.semantic.text.primary)
                .bg(theme.semantic.text.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(after),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::DEFAULT_THEME;

    #[test]
    fn highlighted_spans_marks_current_match() {
        let spans = highlighted_spans("Alpha", 1, "lp", &DEFAULT_THEME);

        assert_eq!(spans[0].content.as_ref(), "A");
        assert_eq!(spans[1].content.as_ref(), "lp");
        assert_eq!(spans[1].style.bg, Some(DEFAULT_THEME.semantic.text.accent));
        assert_eq!(spans[2].content.as_ref(), "ha");
    }

    #[test]
    fn highlighted_spans_uses_casefold_match_length() {
        let spans = highlighted_spans("Maße", 0, "MASSE", &DEFAULT_THEME);

        assert_eq!(spans[1].content.as_ref(), "Maße");
        assert_eq!(spans[2].content.as_ref(), "");
    }
}
