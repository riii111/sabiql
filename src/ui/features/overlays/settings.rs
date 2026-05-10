use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::settings::{ErBrowserChoice, SettingsSection};
use crate::app::model::shared::theme_id::ThemeId;
use crate::primitives::atoms::{CursorKind, text_cursor_spans_with_kind};
use crate::primitives::molecules::render_modal;
use crate::settings_hints::settings_modal_hint_bar;
use crate::theme::{ThemePalette, palette_for};

const PREVIEW_PANEL_INNER_WIDTH: usize = 28;
const PREVIEW_PANEL_TITLE: &str = " Focused panel ";

pub struct SettingsOverlay;

impl SettingsOverlay {
    pub fn render(frame: &mut Frame, state: &AppState, theme: &ThemePalette) {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(48),
            " Settings ",
            settings_modal_hint_bar(state),
            theme,
        );

        let [sidebar, divider, content] = Layout::horizontal([
            Constraint::Length(18),
            Constraint::Length(1),
            Constraint::Min(24),
        ])
        .areas(inner);

        let sidebar_lines = Self::sidebar_lines(state, theme);
        frame.render_widget(Paragraph::new(sidebar_lines), sidebar);
        let divider_lines = (0..inner.height)
            .map(|_| Line::styled("\u{2502}", theme.modal_border_style()))
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(divider_lines), divider);

        match state.settings.section() {
            SettingsSection::Appearance => Self::render_appearance(frame, content, state, theme),
            SettingsSection::ErDiagram => Self::render_er_diagram(frame, content, state, theme),
        }
    }

    fn sidebar_lines(state: &AppState, theme: &ThemePalette) -> Vec<Line<'static>> {
        let mut lines = vec![Line::raw("")];
        for section in SettingsSection::ALL {
            let selected = state.settings.section() == section;
            let marker = if selected { ">" } else { " " };
            let style = if selected {
                Style::default()
                    .fg(theme.component.navigation.section_header)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.semantic.text.secondary)
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {marker} "),
                    Style::default().fg(theme.component.navigation.active_indicator),
                ),
                Span::styled(section.label(), style),
            ]));
        }
        lines
    }

    fn render_appearance(frame: &mut Frame, content: Rect, state: &AppState, theme: &ThemePalette) {
        let [theme_list, preview] =
            Layout::vertical([Constraint::Min(5), Constraint::Length(8)]).areas(content);

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

        Self::render_theme_preview(frame, preview, palette_for(state.settings.selected_theme()));
    }

    fn render_er_diagram(frame: &mut Frame, content: Rect, state: &AppState, theme: &ThemePalette) {
        let mut lines = vec![
            Line::raw(""),
            Line::from(Span::styled(
                "ER Diagram",
                Style::default()
                    .fg(theme.semantic.text.primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Browser",
                Style::default().fg(theme.semantic.text.primary),
            )),
            Line::raw(""),
        ];

        for choice in ErBrowserChoice::ALL {
            if choice == ErBrowserChoice::Custom {
                continue;
            }
            let selected = state.settings.selected_er_browser_choice() == choice;
            let saved = saved_browser_matches(state, choice);
            let marker = if selected { ">" } else { " " };
            let saved_label = if saved { " saved" } else { "" };
            let style = if selected {
                theme.picker_selected_style()
            } else {
                Style::default().fg(theme.semantic.text.secondary)
            };
            lines.push(Line::from(Span::styled(
                format!("  {marker} {:<18}{saved_label}", choice.label()),
                style,
            )));
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Custom command",
            Style::default().fg(theme.semantic.text.primary),
        )));
        let custom_selected =
            state.settings.selected_er_browser_choice() == ErBrowserChoice::Custom;
        let custom_saved = saved_browser_matches(state, ErBrowserChoice::Custom);
        let marker = if custom_selected { ">" } else { " " };
        let saved_label = if custom_saved { " saved" } else { "" };
        let style = if custom_selected {
            theme.picker_selected_style()
        } else {
            Style::default().fg(theme.semantic.text.secondary)
        };
        let mut spans = vec![Span::styled(format!("  {marker} ["), style)];
        let edit_label = if state.settings.is_editing_custom_er_browser() {
            " editing"
        } else {
            ""
        };
        let suffix = format!("]{saved_label}{edit_label}");
        let input_width = usize::from(content.width)
            .saturating_sub(5 + suffix.chars().count())
            .max(1);
        spans.extend(text_cursor_spans_with_kind(
            state.settings.custom_er_browser().content(),
            state.settings.custom_er_browser().cursor(),
            state.settings.custom_er_browser().viewport_offset(),
            input_width,
            CursorKind::Insert,
            theme,
        ));
        spans.push(Span::styled(suffix, style));
        lines.push(Line::from(spans));

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), content);
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
        let border_style = Style::default().fg(theme.semantic.surface.focus_border);
        let panel_title_width = PREVIEW_PANEL_TITLE.chars().count();
        debug_assert!(panel_title_width < PREVIEW_PANEL_INNER_WIDTH);
        let title_rule_width = PREVIEW_PANEL_INNER_WIDTH
            .saturating_sub(panel_title_width)
            .max(1);
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
                Span::styled("\u{256d}", border_style),
                Span::styled(PREVIEW_PANEL_TITLE, border_style),
                Span::styled("\u{2500}".repeat(title_rule_width), border_style),
                Span::styled("\u{256e}", border_style),
            ]),
            Line::from(vec![
                Span::styled("\u{2502} ", border_style),
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
                Span::styled("\u{2502}", border_style),
            ]),
            Line::from(vec![
                Span::styled("\u{2570}", border_style),
                Span::styled("\u{2500}".repeat(PREVIEW_PANEL_INNER_WIDTH), border_style),
                Span::styled("\u{256f}", border_style),
            ]),
            Line::styled(
                "Choose a theme that fits your terminal.",
                Style::default().fg(theme.semantic.text.secondary),
            ),
        ];

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }
}

fn saved_browser_matches(state: &AppState, choice: ErBrowserChoice) -> bool {
    ErBrowserChoice::from_browser_name(state.settings.saved_er_browser()) == choice
}
