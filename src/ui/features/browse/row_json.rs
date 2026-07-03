use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::flash_timer::FlashId;
use crate::primitives::atoms::apply_yank_flash;
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::theme::ThemePalette;

pub struct RowJson;

impl RowJson {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        now: std::time::Instant,
        theme: &ThemePalette,
    ) -> Option<usize> {
        if !state.row_json.is_active() {
            return None;
        }

        let title = " Row JSON ";
        let hints = vec![
            ("y", "Copy"),
            ("j/k", "Scroll"),
            ("g/G", "Jump"),
            (":", "Jump line"),
            ("Esc", "Close"),
        ];

        let (_area, inner) = render_modal(
            frame,
            Constraint::Percentage(80),
            Constraint::Percentage(70),
            title,
            FooterHintBar::new(hints),
            theme,
        );

        let content = state.row_json.content();
        let scroll_offset = state.row_json.scroll_offset();
        let mut lines: Vec<Line> = content
            .lines()
            .map(|line| {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(theme.semantic.text.primary),
                ))
            })
            .collect();

        let flash_active = state.flash_timers.is_active(FlashId::RowJson, now);
        apply_yank_flash(&mut lines, flash_active, theme);

        let paragraph = Paragraph::new(lines)
            .scroll((scroll_offset as u16, 0))
            .style(Style::default().fg(theme.semantic.text.primary));

        frame.render_widget(paragraph, inner);
        Some(inner.height as usize)
    }
}
