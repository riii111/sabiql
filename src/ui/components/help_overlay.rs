use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::keybindings::{
    COMMAND_LINE_KEYS, CONFIRM_DIALOG_KEYS, CONNECTION_ERROR_KEYS, CONNECTION_SETUP_KEYS,
    GLOBAL_KEYS, NAVIGATION_KEYS, OVERLAY_KEYS, SQL_MODAL_KEYS,
};
use crate::app::state::AppState;
use crate::ui::theme::Theme;

use super::molecules::render_modal;
use super::scroll_indicator::{VerticalScrollParams, render_vertical_scroll_indicator_bar};

pub struct HelpOverlay;

impl HelpOverlay {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(70),
            Constraint::Percentage(80),
            " Help ",
            " j/k / ↑↓ to scroll, ? or Esc to close ",
        );

        let mut help_lines = vec![Self::section("Global Keys")];
        for kb in GLOBAL_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Navigation"));
        for kb in NAVIGATION_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("SQL Editor"));
        for kb in SQL_MODAL_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Overlays"));
        for kb in OVERLAY_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Command Line"));
        for kb in COMMAND_LINE_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Connection Setup"));
        for kb in CONNECTION_SETUP_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Connection Error"));
        for kb in CONNECTION_ERROR_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Confirm Dialog"));
        for kb in CONFIRM_DIALOG_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        let total_lines = help_lines.len();
        let viewport_height = inner.height as usize;
        let max_scroll = total_lines.saturating_sub(viewport_height);
        let scroll_offset = state.ui.help_scroll_offset.min(max_scroll);

        let help = Paragraph::new(help_lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(Theme::MODAL_BG))
            .scroll((scroll_offset as u16, 0));

        frame.render_widget(help, inner);

        render_vertical_scroll_indicator_bar(
            frame,
            inner,
            VerticalScrollParams {
                position: scroll_offset,
                viewport_size: viewport_height,
                total_items: total_lines,
            },
        );
    }

    fn section(title: &str) -> Line<'static> {
        Line::from(vec![
            Span::styled("▸ ", Style::default().fg(Color::Cyan)),
            Span::styled(
                title.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    }

    fn key_line(key: &str, desc: &str) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                format!("  {:<13}", key),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(desc.to_string(), Style::default().fg(Color::Gray)),
        ])
    }
}
