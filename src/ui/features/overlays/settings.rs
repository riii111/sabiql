use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::theme_id::ThemeId;
use crate::primitives::molecules::render_modal;
use crate::theme::{ThemePalette, palette_for};

pub struct SettingsOverlay;

impl SettingsOverlay {
    pub fn render(frame: &mut Frame, state: &AppState, theme: &ThemePalette) {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(48),
            " Settings ",
            " Enter Apply │ Esc Cancel ",
            theme,
        );

        let [sidebar, content] =
            Layout::horizontal([Constraint::Length(18), Constraint::Min(24)]).areas(inner);
        let [theme_list, preview] =
            Layout::vertical([Constraint::Min(5), Constraint::Length(8)]).areas(content);

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
            theme_list,
        );

        render_theme_preview(frame, preview, palette_for(state.settings.selected_theme()));
    }
}

fn render_theme_preview(frame: &mut Frame, area: Rect, theme: &ThemePalette) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(theme.modal_border_style())
        .title(Span::styled(
            " Preview ",
            Style::default().fg(theme.component.modal.title),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let selected_style = theme.picker_selected_style();
    let lines = vec![
        Line::from(vec![
            Span::styled(
                "> Active item",
                Style::default().fg(theme.semantic.text.primary),
            ),
            Span::raw("        "),
            Span::styled("Selected row", selected_style),
        ]),
        Line::from(vec![
            Span::styled(
                "  Secondary text",
                Style::default().fg(theme.semantic.text.secondary),
            ),
            Span::raw("     "),
            Span::styled("Muted text", Style::default().fg(theme.semantic.text.muted)),
        ]),
        Line::from(vec![
            Span::styled(
                "\u{256d} Focused panel ",
                Style::default().fg(theme.semantic.surface.focus_border),
            ),
            Span::styled(
                "\u{2500}".repeat(12),
                Style::default().fg(theme.semantic.surface.focus_border),
            ),
            Span::styled(
                "\u{256e}",
                Style::default().fg(theme.semantic.surface.focus_border),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "\u{2502} ",
                Style::default().fg(theme.semantic.surface.focus_border),
            ),
            Span::styled(
                "Primary text",
                Style::default().fg(theme.semantic.text.primary),
            ),
            Span::raw("  "),
            Span::styled(
                "Accent text",
                Style::default().fg(theme.semantic.text.accent),
            ),
            Span::raw("  "),
            Span::styled(
                "\u{2502}",
                Style::default().fg(theme.semantic.surface.focus_border),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "\u{2570}",
                Style::default().fg(theme.semantic.surface.focus_border),
            ),
            Span::styled(
                "\u{2500}".repeat(28),
                Style::default().fg(theme.semantic.surface.focus_border),
            ),
            Span::styled(
                "\u{256f}",
                Style::default().fg(theme.semantic.surface.focus_border),
            ),
        ]),
        Line::styled(
            "Choose a theme that fits your terminal.",
            Style::default().fg(theme.semantic.text.secondary),
        ),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
