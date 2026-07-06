use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::flash_timer::FlashId;
use crate::app::update::input::keybindings::row_detail as row_detail_keys;
use crate::primitives::atoms::apply_yank_flash;
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::theme::ThemePalette;

pub struct RowDetail;

impl RowDetail {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        now: std::time::Instant,
        theme: &ThemePalette,
    ) -> Option<usize> {
        if !state.row_detail.is_active() {
            return None;
        }

        let title = " Row Detail ";
        let hints = [
            row_detail_keys::YANK.as_hint(),
            row_detail_keys::YANK_JSON.as_hint(),
            row_detail_keys::SCROLL.as_hint(),
            row_detail_keys::JUMP.as_hint(),
            row_detail_keys::CLOSE.as_hint(),
        ];

        let (_area, inner) = render_modal(
            frame,
            Constraint::Percentage(80),
            Constraint::Percentage(70),
            title,
            FooterHintBar::new(hints),
            theme,
        );

        let content = state.row_detail.content();
        let scroll_offset = state.row_detail.scroll_offset();
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
            .scroll((scroll_offset as u16, 0))
            .style(Style::default().fg(theme.semantic.text.primary));

        frame.render_widget(paragraph, inner);
        Some(inner.height as usize)
    }
}
