use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::theme_id::ThemeId;
use crate::primitives::molecules::render_modal;
use crate::theme::ThemePalette;

pub struct SettingsOverlay;

impl SettingsOverlay {
    pub fn render(frame: &mut Frame, state: &AppState, theme: &ThemePalette) {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(72),
            Constraint::Percentage(64),
            " Settings ",
            " Enter Apply │ Esc Cancel ",
            theme,
        );

        let [sidebar, content] =
            Layout::horizontal([Constraint::Length(18), Constraint::Min(24)]).areas(inner);

        let sidebar_lines = vec![
            Line::raw(""),
            Line::from(vec![
                Span::styled(
                    "  > ",
                    Style::default().fg(theme.component.navigation.active_indicator),
                ),
                Span::styled(
                    "Appearance",
                    Style::default()
                        .fg(theme.component.navigation.section_header)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        ];
        frame.render_widget(Paragraph::new(sidebar_lines), sidebar);

        let mut content_lines = vec![
            Line::raw(""),
            Line::from(Span::styled(
                "Theme",
                Style::default()
                    .fg(theme.semantic.text.primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
        ];

        for theme_id in ThemeId::ALL {
            let selected = state.settings.selected_theme() == theme_id;
            let saved = state.settings.previous_theme() == theme_id;
            let marker = if selected { ">" } else { " " };
            let saved_label = if saved { " saved" } else { "" };
            let style = if selected {
                theme.picker_selected_style()
            } else {
                Style::default().fg(theme.semantic.text.secondary)
            };
            content_lines.push(Line::from(Span::styled(
                format!("  {marker} {:<14}{saved_label}", theme_id.label()),
                style,
            )));
        }

        frame.render_widget(
            Paragraph::new(content_lines).wrap(Wrap { trim: false }),
            content,
        );
    }
}
