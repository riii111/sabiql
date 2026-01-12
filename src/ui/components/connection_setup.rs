use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::molecules::render_modal;
use crate::app::connection_setup_state::{
    CONNECTION_INPUT_VISIBLE_WIDTH, CONNECTION_INPUT_WIDTH, ConnectionField, ConnectionSetupState,
};
use crate::app::state::AppState;
use crate::domain::connection::SslMode;
use crate::ui::theme::Theme;

const LABEL_WIDTH: u16 = 12;
const INPUT_WIDTH: u16 = CONNECTION_INPUT_WIDTH;
const ERROR_WIDTH: u16 = 12;
const FIELD_HEIGHT: u16 = 1;
const DROPDOWN_ITEM_COUNT: usize = 6;

fn bracketed_input(content: &str, border_style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled("[", border_style),
        Span::styled(format!(" {} ", content), Style::default().fg(Color::White)),
        Span::styled("]", border_style),
    ])
}

pub struct ConnectionSetup;

impl ConnectionSetup {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let form_state = &state.connection_setup;

        let modal_width = LABEL_WIDTH + INPUT_WIDTH + ERROR_WIDTH + 8;
        let modal_height = 13;

        let (title, hint) = if form_state.is_edit_mode() {
            (
                " Edit Connection ",
                " Tab: Next │ Shift+Tab: Prev │ Ctrl+S: Save │ Esc: Cancel ",
            )
        } else {
            (
                " New Connection ",
                " Tab: Next │ Shift+Tab: Prev │ Ctrl+S: Connect │ Esc: Cancel ",
            )
        };
        let (_, modal_inner) = render_modal(
            frame,
            Constraint::Length(modal_width),
            Constraint::Length(modal_height),
            title,
            hint,
        );

        let inner = modal_inner.inner(Margin::new(2, 1));
        let chunks = Layout::vertical([
            Constraint::Length(FIELD_HEIGHT), // Name
            Constraint::Length(FIELD_HEIGHT), // Host
            Constraint::Length(FIELD_HEIGHT), // Port
            Constraint::Length(FIELD_HEIGHT), // Database
            Constraint::Length(FIELD_HEIGHT), // User
            Constraint::Length(FIELD_HEIGHT), // Password
            Constraint::Length(FIELD_HEIGHT), // SslMode
            Constraint::Length(1),            // spacer
            Constraint::Length(1),            // notice
        ])
        .split(inner);

        Self::render_text_field(frame, chunks[0], form_state, ConnectionField::Name, false);
        Self::render_text_field(frame, chunks[1], form_state, ConnectionField::Host, false);
        Self::render_text_field(frame, chunks[2], form_state, ConnectionField::Port, false);
        Self::render_text_field(
            frame,
            chunks[3],
            form_state,
            ConnectionField::Database,
            false,
        );
        Self::render_text_field(frame, chunks[4], form_state, ConnectionField::User, false);
        Self::render_text_field(
            frame,
            chunks[5],
            form_state,
            ConnectionField::Password,
            true,
        );
        Self::render_ssl_field(
            frame,
            chunks[6],
            form_state.ssl_mode,
            form_state.focused_field == ConnectionField::SslMode,
        );

        let notice = "Note: Connection info is stored locally in plain text";
        let notice_para = Paragraph::new(notice).style(Style::default().fg(Theme::NOTE_TEXT));
        frame.render_widget(notice_para, chunks[8]);

        if form_state.ssl_dropdown.is_open {
            Self::render_dropdown(frame, chunks[6], form_state.ssl_dropdown.selected_index);
        }
    }

    fn render_text_field(
        frame: &mut Frame,
        area: Rect,
        state: &ConnectionSetupState,
        field: ConnectionField,
        mask: bool,
    ) {
        let is_focused = field == state.focused_field;
        let value = state.field_value(field);
        let error = state.validation_errors.get(&field);

        let chunks = Layout::horizontal([
            Constraint::Length(LABEL_WIDTH),
            Constraint::Length(INPUT_WIDTH),
            Constraint::Length(ERROR_WIDTH),
        ])
        .split(area);

        let label_style = if is_focused {
            Style::default().fg(Color::Gray).bold()
        } else {
            Style::default().fg(Color::Gray)
        };
        let label_para = Paragraph::new(field.label()).style(label_style);
        frame.render_widget(label_para, chunks[0]);

        let display_value = if mask {
            "*".repeat(value.chars().count())
        } else {
            value.to_string()
        };

        let content_width = CONNECTION_INPUT_VISIBLE_WIDTH;
        let input_content = if is_focused {
            let viewport = state.viewport_offset;
            let cursor = state.cursor_position;
            let char_count = display_value.chars().count();
            let visible_chars = content_width - 1; // Reserve 1 char for cursor
            let visible_end = (viewport + visible_chars).min(char_count);
            let visible_start = viewport.min(char_count);
            let visible_text: String = display_value
                .chars()
                .skip(visible_start)
                .take(visible_end - visible_start)
                .collect();
            let cursor_in_visible = cursor.saturating_sub(viewport);
            let before_cursor: String = visible_text.chars().take(cursor_in_visible).collect();
            let after_cursor: String = visible_text.chars().skip(cursor_in_visible).collect();
            let with_cursor = format!("{}█{}", before_cursor, after_cursor);
            format!("{:<1$}", with_cursor, content_width)
        } else {
            let truncated: String = display_value.chars().take(content_width).collect();
            format!("{:<1$}", truncated, content_width)
        };

        let border_style = if error.is_some() {
            Style::default().fg(Color::Red)
        } else if is_focused {
            Style::default().fg(Theme::MODAL_BORDER_HIGHLIGHT)
        } else {
            Style::default().fg(Theme::MODAL_BORDER)
        };

        let input_para = Paragraph::new(bracketed_input(&input_content, border_style));
        frame.render_widget(input_para, chunks[1]);

        if let Some(err) = error {
            let err_para =
                Paragraph::new(format!(" {}", err)).style(Style::default().fg(Color::Red));
            frame.render_widget(err_para, chunks[2]);
        }
    }

    fn render_ssl_field(frame: &mut Frame, area: Rect, ssl_mode: SslMode, is_focused: bool) {
        let chunks = Layout::horizontal([
            Constraint::Length(LABEL_WIDTH),
            Constraint::Length(INPUT_WIDTH),
            Constraint::Length(ERROR_WIDTH),
        ])
        .split(area);

        // Label: gray (like Explorer content), bold when focused
        let label_style = if is_focused {
            Style::default().fg(Color::Gray).bold()
        } else {
            Style::default().fg(Color::Gray)
        };
        let label_para = Paragraph::new("SSL Mode:").style(label_style);
        frame.render_widget(label_para, chunks[0]);

        // Value: white (emphasized), same width as text fields
        let content_width = CONNECTION_INPUT_VISIBLE_WIDTH;
        let ssl_mode_str = ssl_mode.to_string();
        let display_content = format!("{:<1$} ▼", ssl_mode_str, content_width - 2);

        let border_style = if is_focused {
            Style::default().fg(Theme::MODAL_BORDER_HIGHLIGHT)
        } else {
            Style::default().fg(Theme::MODAL_BORDER)
        };

        let input_para = Paragraph::new(bracketed_input(&display_content, border_style));
        frame.render_widget(input_para, chunks[1]);
    }

    fn render_dropdown(frame: &mut Frame, ssl_field_area: Rect, selected_index: usize) {
        let chunks = Layout::horizontal([
            Constraint::Length(LABEL_WIDTH),
            Constraint::Length(INPUT_WIDTH),
            Constraint::Length(ERROR_WIDTH),
        ])
        .split(ssl_field_area);

        let dropdown_area = Rect {
            x: chunks[1].x,
            y: chunks[1].y + 1,
            width: INPUT_WIDTH,
            height: DROPDOWN_ITEM_COUNT as u16 + 2,
        };

        frame.render_widget(Clear, dropdown_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .style(Style::default().bg(Theme::MODAL_BG));
        frame.render_widget(block, dropdown_area);

        let inner = dropdown_area.inner(Margin::new(1, 1));
        let variants = SslMode::all_variants();

        for (i, variant) in variants.iter().enumerate() {
            if i >= DROPDOWN_ITEM_COUNT {
                break;
            }
            let item_area = Rect {
                x: inner.x,
                y: inner.y + i as u16,
                width: inner.width,
                height: 1,
            };

            let is_selected = i == selected_index;
            let item_style = if is_selected {
                Style::default()
                    .bg(Theme::COMPLETION_SELECTED_BG)
                    .fg(Color::White)
            } else {
                Style::default().fg(Color::Gray)
            };

            let item_para = Paragraph::new(variant.to_string()).style(item_style);
            frame.render_widget(item_para, item_area);
        }
    }
}
