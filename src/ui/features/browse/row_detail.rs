use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::flash_timer::FlashId;
use crate::app::model::shared::render_output::RowDetailRenderMetrics;
use crate::app::update::input::keybindings::{ModeRow, ROW_DETAIL_FOOTER_ROWS};
use crate::primitives::atoms::apply_yank_flash;
use crate::primitives::atoms::scroll_indicator::{
    HorizontalScrollParams, VerticalScrollParams, clamp_scroll_offset,
    render_horizontal_scroll_indicator, render_vertical_scroll_indicator_bar,
};
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::theme::ThemePalette;

pub struct RowDetail;

impl RowDetail {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        now: std::time::Instant,
        theme: &ThemePalette,
    ) -> Option<RowDetailRenderMetrics> {
        if !state.row_detail.is_active() {
            return None;
        }

        let title = " Row Detail ";
        let hints = ROW_DETAIL_FOOTER_ROWS.iter().map(ModeRow::as_hint);

        let (_area, inner) = render_modal(
            frame,
            Constraint::Percentage(80),
            Constraint::Percentage(70),
            title,
            FooterHintBar::new(hints),
            theme,
        );

        let content = state.row_detail.content();
        let total_lines = state.row_detail.line_count();
        let content_width = state.row_detail.content_width();
        let viewport = detail_viewport(inner, total_lines, content_width);
        let scroll_offset = clamp_scroll_offset(
            state.row_detail.scroll_offset(),
            viewport.content_area.height as usize,
            total_lines,
        );
        let horizontal_offset = clamp_scroll_offset(
            state.row_detail.horizontal_offset(),
            viewport.content_area.width as usize,
            content_width,
        );
        let mut lines: Vec<Line> = content
            .lines()
            .map(|line| {
                if line.starts_with("  ") {
                    Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(theme.semantic.text.primary),
                    ))
                } else if line.is_empty() {
                    Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(theme.semantic.text.secondary),
                    ))
                } else {
                    Line::from(Span::styled(
                        line.to_string(),
                        Style::default()
                            .fg(theme.semantic.text.accent)
                            .add_modifier(Modifier::BOLD),
                    ))
                }
            })
            .collect();

        let flash_active = state.flash_timers.is_active(FlashId::RowDetail, now);
        apply_yank_flash(&mut lines, flash_active, theme);

        let paragraph = Paragraph::new(lines)
            .scroll((to_u16(scroll_offset), to_u16(horizontal_offset)))
            .style(Style::default().fg(theme.semantic.text.primary));

        frame.render_widget(paragraph, viewport.content_area);

        if viewport.has_horizontal_scrollbar {
            render_horizontal_scroll_indicator(
                frame,
                inner,
                HorizontalScrollParams {
                    position: horizontal_offset,
                    viewport_size: viewport.content_area.width as usize,
                    total_items: content_width,
                    label: "x",
                },
                theme,
            );
        }

        if viewport.has_vertical_scrollbar {
            render_vertical_scroll_indicator_bar(
                frame,
                inner,
                VerticalScrollParams {
                    position: scroll_offset,
                    viewport_size: viewport.content_area.height as usize,
                    total_items: total_lines,
                    has_horizontal_scrollbar: viewport.has_horizontal_scrollbar,
                },
                theme,
            );
        }

        Some(RowDetailRenderMetrics {
            visible_rows: viewport.content_area.height as usize,
            visible_columns: viewport.content_area.width as usize,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct DetailViewport {
    content_area: Rect,
    has_horizontal_scrollbar: bool,
    has_vertical_scrollbar: bool,
}

fn detail_viewport(area: Rect, total_lines: usize, content_width: usize) -> DetailViewport {
    let mut content_area = area;
    let mut has_horizontal_scrollbar = false;
    let mut has_vertical_scrollbar = false;

    // Scrollbar visibility is mutually dependent through content_area shrinkage.
    // Two monotonic passes are enough because each flag can only move toward true.
    for _ in 0..2 {
        has_horizontal_scrollbar = content_width > content_area.width as usize;
        has_vertical_scrollbar = total_lines > content_area.height as usize;
        content_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width.saturating_sub(u16::from(has_vertical_scrollbar)),
            height: area
                .height
                .saturating_sub(u16::from(has_horizontal_scrollbar)),
        };
    }

    DetailViewport {
        content_area,
        has_horizontal_scrollbar,
        has_vertical_scrollbar,
    }
}

fn to_u16(value: usize) -> u16 {
    value.min(u16::MAX as usize) as u16
}
