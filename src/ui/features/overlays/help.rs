use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::catalog::{HelpEntry, help_sections};
use crate::ui::theme::ThemePalette;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::ui_state::HELP_MODAL_HEIGHT_PERCENT;

use crate::ui::primitives::atoms::scroll_indicator::{
    VerticalScrollParams, clamp_scroll_offset, render_vertical_scroll_indicator_bar,
};
use crate::ui::primitives::molecules::render_modal;

pub struct HelpOverlay;

impl HelpOverlay {
    pub fn render(frame: &mut Frame, state: &AppState, theme: &ThemePalette) {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(70),
            Constraint::Percentage(HELP_MODAL_HEIGHT_PERCENT),
            " Help ",
            " ?/Esc Close ",
            theme,
        );

        let mut help_lines = Vec::new();
        for (index, section) in help_sections().into_iter().enumerate() {
            if index > 0 {
                help_lines.push(Line::from(""));
            }
            help_lines.push(Self::section(section.title, theme));
            for entry in section.entries {
                help_lines.push(Self::key_line(entry, theme));
            }
        }

        let total_lines = help_lines.len();
        let viewport_height = inner.height as usize;
        let scroll_offset =
            clamp_scroll_offset(state.ui.help_scroll_offset, viewport_height, total_lines);

        let help = Paragraph::new(help_lines)
            .wrap(Wrap { trim: false })
            .style(Style::default())
            .scroll((scroll_offset as u16, 0));

        frame.render_widget(help, inner);

        render_vertical_scroll_indicator_bar(
            frame,
            inner,
            VerticalScrollParams {
                position: scroll_offset,
                viewport_size: viewport_height,
                total_items: total_lines,
                has_horizontal_scrollbar: false,
            },
            theme,
        );
    }

    fn section(title: &str, theme: &ThemePalette) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                "▸ ",
                Style::default().fg(theme.component.navigation.section_header),
            ),
            Span::styled(
                title.to_string(),
                Style::default()
                    .fg(theme.component.navigation.section_header)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    }

    fn key_line(entry: HelpEntry, theme: &ThemePalette) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", entry.key),
                Style::default()
                    .fg(theme.semantic.text.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                entry.description.into_owned(),
                Style::default().fg(theme.semantic.text.secondary),
            ),
        ])
    }
}
