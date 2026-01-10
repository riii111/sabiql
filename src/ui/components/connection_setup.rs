use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::overlay::{centered_rect, modal_block_with_hint, render_scrim};
use crate::app::connection_setup_state::ConnectionField;
use crate::app::state::AppState;
use crate::domain::connection::SslMode;
use crate::ui::theme::Theme;

const LABEL_WIDTH: u16 = 12;
const INPUT_WIDTH: u16 = 40;
const FIELD_HEIGHT: u16 = 1;
const DROPDOWN_ITEM_COUNT: usize = 6;

pub struct ConnectionSetup;

impl ConnectionSetup {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let form_state = &state.connection_setup;

        let modal_width = LABEL_WIDTH + INPUT_WIDTH + 6;
        let modal_height = 14;

        let area = centered_rect(
            frame.area(),
            Constraint::Length(modal_width),
            Constraint::Length(modal_height),
        );
        render_scrim(frame);
        frame.render_widget(Clear, area);

        let hint = " Tab: Next │ Shift+Tab: Prev │ Ctrl+S: Save │ Esc: Cancel ";
        let block = modal_block_with_hint(" Connection Setup ".to_string(), hint.to_string());
        frame.render_widget(block, area);

        let inner = area.inner(Margin::new(2, 1));
        let chunks = Layout::vertical([
            Constraint::Length(FIELD_HEIGHT),
            Constraint::Length(FIELD_HEIGHT),
            Constraint::Length(FIELD_HEIGHT),
            Constraint::Length(FIELD_HEIGHT),
            Constraint::Length(FIELD_HEIGHT),
            Constraint::Length(FIELD_HEIGHT),
            Constraint::Length(1), // spacer
            Constraint::Length(1), // auto name
            Constraint::Length(1), // spacer
            Constraint::Length(1), // notice
        ])
        .split(inner);

        Self::render_text_field(
            frame,
            chunks[0],
            ConnectionField::Host,
            &form_state.host,
            form_state.focused_field,
            form_state.validation_errors.get(&ConnectionField::Host),
            false,
        );
        Self::render_text_field(
            frame,
            chunks[1],
            ConnectionField::Port,
            &form_state.port,
            form_state.focused_field,
            form_state.validation_errors.get(&ConnectionField::Port),
            false,
        );
        Self::render_text_field(
            frame,
            chunks[2],
            ConnectionField::Database,
            &form_state.database,
            form_state.focused_field,
            form_state.validation_errors.get(&ConnectionField::Database),
            false,
        );
        Self::render_text_field(
            frame,
            chunks[3],
            ConnectionField::User,
            &form_state.user,
            form_state.focused_field,
            form_state.validation_errors.get(&ConnectionField::User),
            false,
        );
        Self::render_text_field(
            frame,
            chunks[4],
            ConnectionField::Password,
            &form_state.password,
            form_state.focused_field,
            form_state.validation_errors.get(&ConnectionField::Password),
            true,
        );
        Self::render_ssl_field(
            frame,
            chunks[5],
            form_state.ssl_mode,
            form_state.focused_field == ConnectionField::SslMode,
        );

        let auto_name = format!("Name (auto): {}", form_state.auto_name());
        let auto_name_para =
            Paragraph::new(auto_name).style(Style::default().fg(Theme::MODAL_HINT));
        frame.render_widget(auto_name_para, chunks[7]);

        let notice = "Note: Connection info is stored locally in plain text";
        let notice_para = Paragraph::new(notice)
            .style(Style::default().fg(Theme::MODAL_HINT).dim());
        frame.render_widget(notice_para, chunks[9]);

        if form_state.ssl_dropdown.is_open {
            Self::render_dropdown(frame, chunks[5], form_state.ssl_dropdown.selected_index);
        }
    }

    fn render_text_field(
        frame: &mut Frame,
        area: Rect,
        field: ConnectionField,
        value: &str,
        focused: ConnectionField,
        error: Option<&String>,
        mask: bool,
    ) {
        let is_focused = field == focused;
        let label = field.label();

        let chunks = Layout::horizontal([
            Constraint::Length(LABEL_WIDTH),
            Constraint::Length(INPUT_WIDTH),
            Constraint::Min(0),
        ])
        .split(area);

        let label_style = if is_focused {
            Style::default().fg(Color::White).bold()
        } else {
            Style::default().fg(Theme::MODAL_HINT)
        };
        let label_para = Paragraph::new(label).style(label_style);
        frame.render_widget(label_para, chunks[0]);

        let display_value = if mask {
            "*".repeat(value.len())
        } else {
            value.to_string()
        };

        let input_content = if is_focused {
            format!("{}█", display_value)
        } else {
            display_value
        };

        let input_style = if is_focused {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };

        let border_style = if error.is_some() {
            Style::default().fg(Color::Red)
        } else if is_focused {
            Style::default().fg(Theme::MODAL_BORDER_HIGHLIGHT)
        } else {
            Style::default().fg(Theme::MODAL_BORDER)
        };

        let input_block = Block::default()
            .borders(Borders::NONE)
            .style(border_style);
        let input_para = Paragraph::new(format!("[ {} ]", input_content))
            .style(input_style)
            .block(input_block);
        frame.render_widget(input_para, chunks[1]);

        if let Some(err) = error {
            let err_para = Paragraph::new(format!(" {}", err)).style(Style::default().fg(Color::Red));
            frame.render_widget(err_para, chunks[2]);
        }
    }

    fn render_ssl_field(frame: &mut Frame, area: Rect, ssl_mode: SslMode, is_focused: bool) {
        let chunks = Layout::horizontal([
            Constraint::Length(LABEL_WIDTH),
            Constraint::Length(INPUT_WIDTH),
            Constraint::Min(0),
        ])
        .split(area);

        let label_style = if is_focused {
            Style::default().fg(Color::White).bold()
        } else {
            Style::default().fg(Theme::MODAL_HINT)
        };
        let label_para = Paragraph::new("SSL Mode:").style(label_style);
        frame.render_widget(label_para, chunks[0]);

        let display = format!("[ {} ▼ ]", ssl_mode);
        let input_style = if is_focused {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };
        let input_para = Paragraph::new(display).style(input_style);
        frame.render_widget(input_para, chunks[1]);
    }

    fn render_dropdown(frame: &mut Frame, ssl_field_area: Rect, selected_index: usize) {
        let chunks = Layout::horizontal([
            Constraint::Length(LABEL_WIDTH),
            Constraint::Length(INPUT_WIDTH),
            Constraint::Min(0),
        ])
        .split(ssl_field_area);

        let dropdown_area = Rect {
            x: chunks[1].x,
            y: chunks[1].y + 1,
            width: 20,
            height: DROPDOWN_ITEM_COUNT as u16 + 2,
        };

        frame.render_widget(Clear, dropdown_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::MODAL_BORDER_HIGHLIGHT))
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

            let style = if i == selected_index {
                Style::default()
                    .bg(Theme::COMPLETION_SELECTED_BG)
                    .fg(Color::White)
            } else {
                Style::default().fg(Color::Gray)
            };

            let item = Paragraph::new(variant.to_string()).style(style);
            frame.render_widget(item, item_area);
        }
    }
}
