use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::state::AppState;

use super::overlay::centered_rect;

pub struct HelpOverlay;

impl HelpOverlay {
    pub fn render(frame: &mut Frame, _state: &AppState) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(70),
            Constraint::Percentage(80),
        );

        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Help (press ? or Esc to close) ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(0x1e, 0x1e, 0x2e)));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let help_lines = vec![
            Line::from(vec![Span::styled(
                "=== Global Keys ===",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Self::key_line("q", "Quit application"),
            Self::key_line("?", "Toggle this help"),
            Self::key_line("Ctrl+P", "Open Table Picker"),
            Self::key_line("Ctrl+K", "Open Command Palette"),
            Self::key_line(":", "Enter command line"),
            Self::key_line("f", "Toggle Focus mode (Result fullscreen)"),
            Self::key_line("1/2/3", "Switch pane focus (exits Focus first)"),
            Self::key_line("[ / ]", "Inspector prev/next tab"),
            Self::key_line("r", "Reload metadata"),
            Self::key_line("Tab", "Next tab"),
            Self::key_line("Shift+Tab", "Previous tab"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "=== Navigation ===",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Self::key_line("j / ↓", "Move down (scroll in Result/Focus)"),
            Self::key_line("k / ↑", "Move up (scroll in Result/Focus)"),
            Self::key_line("g / Home", "First item (top in Result/Focus)"),
            Self::key_line("G / End", "Last item (bottom in Result/Focus)"),
            Self::key_line("h / l", "Scroll left/right (Result/Focus only)"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "=== SQL Editor ===",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Self::key_line("Ctrl+Enter", "Execute query"),
            Self::key_line("Esc", "Close editor"),
            Self::key_line("↑↓←→", "Move cursor"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "=== Overlays ===",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Self::key_line("Esc", "Close overlay / Cancel"),
            Self::key_line("Enter", "Confirm selection (Explorer/Picker)"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "=== Command Line ===",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Self::key_line(":quit", "Quit application"),
            Self::key_line(":help", "Show this help"),
            Self::key_line(":sql", "Open SQL Editor"),
            Self::key_line(":console", "Open Console (pgcli)"),
        ];

        let help = Paragraph::new(help_lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(Color::Rgb(0x1e, 0x1e, 0x2e)));

        frame.render_widget(help, inner);
    }

    fn key_line(key: &str, desc: &str) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                format!("{:<15}", key),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(desc.to_string()),
        ])
    }
}
