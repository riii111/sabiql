use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

use super::molecules::render_modal;

pub struct HelpOverlay;

impl HelpOverlay {
    pub fn render(frame: &mut Frame, _state: &AppState) {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(70),
            Constraint::Percentage(80),
            " Help ",
            " ? or Esc to close ",
        );

        let help_lines = vec![
            Self::section("Global Keys"),
            Self::chip_line("q", "Quit application"),
            Self::chip_line("?", "Toggle this help"),
            Self::chip_line("Ctrl+P", "Open Table Picker"),
            Self::chip_line("Ctrl+K", "Open Command Palette"),
            Self::chip_line(":", "Enter command line"),
            Self::chip_line("f", "Toggle Focus mode (Result fullscreen)"),
            Self::key_line("1/2/3", "Switch pane focus (exits Focus first)"),
            Self::key_line("Tab/⇧Tab", "Inspector prev/next tab (Inspector focus)"),
            Self::key_line("r", "Reload metadata"),
            Self::key_line("e", "Open ER Diagram in browser"),
            Self::key_line("c", "Open connection settings"),
            Line::from(""),
            Self::section("Navigation"),
            Self::key_line("j / ↓", "Move down (scroll in Result/Focus)"),
            Self::key_line("k / ↑", "Move up (scroll in Result/Focus)"),
            Self::key_line("g / Home", "First item (top in Result/Focus)"),
            Self::key_line("G / End", "Last item (bottom in Result/Focus)"),
            Self::key_line("h / l", "Scroll left/right (Result/Focus only)"),
            Line::from(""),
            Self::section("SQL Editor"),
            Self::key_line("Alt+Enter", "Execute query"),
            Self::chip_line("Esc", "Close editor"),
            Self::key_line("↑↓←→", "Move cursor"),
            Line::from(""),
            Self::section("Overlays"),
            Self::chip_line("Esc", "Close overlay / Cancel"),
            Self::chip_line("Enter", "Confirm selection (Explorer/Picker)"),
            Line::from(""),
            Self::section("Command Line"),
            Self::key_line(":quit", "Quit application"),
            Self::key_line(":help", "Show this help"),
            Self::key_line(":sql", "Open SQL Editor"),
            Line::from(""),
            Self::section("Connection Setup"),
            Self::key_line("Tab/⇧Tab", "Next/Previous field"),
            Self::chip_line("Ctrl+S", "Save and connect"),
            Self::chip_line("Esc", "Cancel"),
            Line::from(""),
            Self::section("Connection Error"),
            Self::key_line("r", "Retry connection"),
            Self::key_line("e", "Edit connection settings"),
            Self::key_line("d", "Toggle error details"),
            Self::key_line("c", "Copy error to clipboard"),
        ];

        let help = Paragraph::new(help_lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(Theme::MODAL_BG));

        frame.render_widget(help, inner);
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

    fn chip_line(key: &str, desc: &str) -> Line<'static> {
        let chip = format!(" {} ", key);
        let padding_len = 15usize.saturating_sub(chip.len() + 2);
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                chip,
                Style::default()
                    .bg(Theme::KEY_CHIP_BG)
                    .fg(Theme::KEY_CHIP_FG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" ".repeat(padding_len)),
            Span::styled(desc.to_string(), Style::default().fg(Color::Gray)),
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
