use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::model::browse::jsonb_detail::JsonbDetailMode;
use crate::app::model::shared::flash_timer::FlashId;
use crate::app::model::shared::text_input::TextInputLike;
use crate::features::browse::detail_view::render_detail_search;
use crate::primitives::atoms::scroll_indicator::{
    VerticalScrollParams, clamp_scroll_offset, render_vertical_scroll_indicator_bar,
};
use crate::primitives::atoms::{
    CursorKind, ModalTextSurface, apply_yank_flash, build_modal_text_surface_lines,
    render_modal_text_surface,
};
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::theme::ThemePalette;

pub struct JsonbDetailRenderMetrics {
    pub editor_visible_rows: usize,
}
pub struct JsonbDetail;

impl JsonbDetail {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        now: std::time::Instant,
        theme: &ThemePalette,
    ) -> Option<JsonbDetailRenderMetrics> {
        if !state.jsonb_detail.is_active() {
            return None;
        }

        let is_editing = matches!(state.jsonb_detail.mode(), JsonbDetailMode::Editing);
        let title = if is_editing {
            format!(
                " JSONB Edit \u{2500}\u{2500} {} (jsonb) ",
                state.jsonb_detail.column_name()
            )
        } else {
            format!(
                " JSONB Detail \u{2500}\u{2500} {}",
                state.jsonb_detail.column_name()
            )
        };
        let hints = if is_editing {
            vec![("Esc", "Normal")]
        } else {
            vec![
                ("y", "Copy"),
                ("/", "Search"),
                ("i", "Insert"),
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

        let (editor_area, status_area, search_area) = if state.jsonb_detail.search().is_active() {
            let [editor_area, status_area, search_area] = Layout::vertical([
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(inner);
            (editor_area, status_area, Some(search_area))
        } else {
            let [editor_area, status_area] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
            (editor_area, status_area, None)
        };

        Self::render_editor_content(frame, editor_area, state, is_editing, now, theme);
        Self::render_status(frame, status_area, state, theme);
        if let Some(search_area) = search_area {
            Self::render_search(frame, search_area, state, theme);
        }

        Some(JsonbDetailRenderMetrics {
            editor_visible_rows: editor_area.height as usize,
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
        let editor = state.jsonb_detail.editor();
        let content = editor.content();
        let total_lines = content.lines().count().max(1);
        let has_vertical_scrollbar = total_lines > area.height as usize;
        let content_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width.saturating_sub(u16::from(has_vertical_scrollbar)),
            height: area.height,
        };
        let scroll_row =
            clamp_scroll_offset(editor.scroll_row(), area.height as usize, total_lines);
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
            scroll_row,
            cursor_kind,
            empty_placeholder: if is_editing {
                " Enter JSON..."
            } else {
                " Press i to edit..."
            },
            base_style: Style::default().fg(theme.semantic.text.primary),
            current_line_style: Style::default().bg(theme.component.editor.current_line_bg),
        };

        let line_spans: Vec<Vec<Span<'static>>> = content
            .lines()
            .map(|line| vec![Span::raw(line.to_owned())])
            .collect();
        let mut lines = build_modal_text_surface_lines(surface, line_spans, theme);

        let flash_active = state.flash_timers.is_active(FlashId::JsonbDetail, now);
        apply_yank_flash(&mut lines, flash_active, theme);

        render_modal_text_surface(frame, content_area, surface, lines);

        if has_vertical_scrollbar {
            render_vertical_scroll_indicator_bar(
                frame,
                area,
                VerticalScrollParams {
                    position: scroll_row,
                    viewport_size: area.height as usize,
                    total_items: total_lines,
                    has_horizontal_scrollbar: false,
                },
                theme,
            );
        }
    }

    fn render_status(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) {
        let mut spans = Vec::new();

        if state.jsonb_detail.has_pending_changes() {
            spans.push(Span::styled(
                "\u{25cf} Modified  ",
                Style::default().fg(theme.semantic.status.pending),
            ));
        }

        if let Some(err) = state.jsonb_detail.validation_error() {
            spans.push(Span::styled(
                format!("\u{2717} {err}"),
                Style::default().fg(theme.semantic.status.error),
            ));
        } else {
            spans.push(Span::styled(
                "\u{2713} Valid JSON",
                Style::default().fg(theme.semantic.status.success),
            ));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn render_search(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) {
        render_detail_search(frame, area, state.jsonb_detail.search(), theme);
    }
}
