use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

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

        // Clear the background
        frame.render_widget(Clear, area);

        // Outer block
        let block = Block::default()
            .title(" Help (press ? or Esc to close) ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::DarkGray));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Help content
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
            Self::key_line("f", "Toggle Focus mode"),
            Self::key_line("1", "Switch to Browse tab"),
            Self::key_line("2", "Switch to ER tab"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "=== Navigation ===",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Self::key_line("j / ↓", "Move down"),
            Self::key_line("k / ↑", "Move up"),
            Self::key_line("h / ←", "Move left"),
            Self::key_line("l / →", "Move right"),
            Self::key_line("g / Home", "Go to first item"),
            Self::key_line("G / End", "Go to last item"),
            Self::key_line("PgUp", "Page up"),
            Self::key_line("PgDn", "Page down"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "=== Overlays ===",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Self::key_line("Esc", "Close overlay / Cancel"),
            Self::key_line("Enter", "Confirm selection"),
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
            Self::key_line(":sql", "Open SQL Modal (PR4)"),
            Self::key_line(":open-console", "Open Console (PR5)"),
        ];

        let help = Paragraph::new(help_lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(Color::DarkGray));

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
