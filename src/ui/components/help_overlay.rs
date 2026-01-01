use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

use super::overlay::{centered_rect, modal_block_with_hint, render_scrim};

pub struct HelpOverlay;

impl HelpOverlay {
    pub fn render(frame: &mut Frame, _state: &AppState) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(70),
            Constraint::Percentage(80),
        );

        render_scrim(frame);
        frame.render_widget(Clear, area);

        let block = modal_block_with_hint(" Help ".to_string(), " ? or Esc to close ".to_string());
        let inner = block.inner(area);
        frame.render_widget(block, area);

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
            Self::key_line(":console", "Open Console (pgcli)"),
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
