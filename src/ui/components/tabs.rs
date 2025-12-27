use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Tabs as RatatuiTabs;

use crate::app::state::AppState;

pub struct Tabs;

impl Tabs {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let titles = vec!["Browse", "ER"];
        let tabs = RatatuiTabs::new(titles)
            .select(state.active_tab)
            .style(Style::default().fg(Color::White))
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .divider(" | ");

        frame.render_widget(tabs, area);
    }
}
