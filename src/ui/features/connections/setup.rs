use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::model::app_state::AppState;
use crate::app::model::connection::setup::{
    CONNECTION_INPUT_VISIBLE_WIDTH, CONNECTION_INPUT_WIDTH, ConnectionField, ConnectionSetupState,
};
use crate::domain::connection::{DatabaseType, SslMode};
use crate::primitives::atoms::text_cursor_spans;
use crate::primitives::molecules::render_modal;
use crate::theme::ThemePalette;

const LABEL_WIDTH: u16 = 12;
const INPUT_WIDTH: u16 = CONNECTION_INPUT_WIDTH;
const ERROR_WIDTH: u16 = 12;
const FIELD_HEIGHT: u16 = 1;
const SSL_DROPDOWN_ITEM_COUNT: usize = 6;
const DB_TYPE_DROPDOWN_ITEM_COUNT: usize = DatabaseType::all().len();

fn bracketed_input(content: &str, border_style: Style, theme: &ThemePalette) -> Line<'static> {
    Line::from(vec![
        Span::styled("[", border_style),
        Span::styled(
            format!(" {content} "),
            Style::default().fg(theme.semantic.text.primary),
        ),
        Span::styled("]", border_style),
    ])
}

pub struct ConnectionSetup;

impl ConnectionSetup {
    pub fn render(frame: &mut Frame, state: &AppState, theme: &ThemePalette) {
        let form_state = &state.connection_setup;

        let modal_width = LABEL_WIDTH + INPUT_WIDTH + ERROR_WIDTH + 8;
        let modal_height = match form_state.database_type {
            DatabaseType::PostgreSQL => 14,
            DatabaseType::SQLite => 9,
        };

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
            theme,
        );

        let inner = modal_inner.inner(Margin::new(2, 1));
        let field_count = form_state.visible_fields().len();
        let mut constraints = vec![Constraint::Length(FIELD_HEIGHT); field_count];
        constraints.push(Constraint::Length(1));
        constraints.push(Constraint::Length(1));
        let chunks = Layout::vertical(constraints).split(inner);

        for (idx, field) in form_state.visible_fields().iter().enumerate() {
            match field {
                ConnectionField::DatabaseType => Self::render_database_type_field(
                    frame,
                    chunks[idx],
                    form_state.database_type,
                    form_state.focused_field == ConnectionField::DatabaseType,
                    theme,
                ),
                ConnectionField::SslMode => Self::render_ssl_field(
                    frame,
                    chunks[idx],
                    form_state.ssl_mode,
                    form_state.focused_field == ConnectionField::SslMode,
                    theme,
                ),
                field => Self::render_text_field(
                    frame,
                    chunks[idx],
                    form_state,
                    *field,
                    *field == ConnectionField::Password,
                    theme,
                ),
            }
        }

        let notice = "Note: Connection info is stored locally in plain text";
        let notice_para =
            Paragraph::new(notice).style(Style::default().fg(theme.component.feedback.note_text));
        frame.render_widget(notice_para, chunks[field_count + 1]);

        if form_state.database_type_dropdown.is_open {
            Self::render_database_type_dropdown(
                frame,
                chunks[0],
                form_state.database_type_dropdown.selected_index,
                theme,
            );
        } else if form_state.ssl_dropdown.is_open {
            let ssl_idx = form_state
                .visible_fields()
                .iter()
                .position(|field| *field == ConnectionField::SslMode)
                .unwrap_or(0);
            Self::render_dropdown(
                frame,
                chunks[ssl_idx],
                form_state.ssl_dropdown.selected_index,
                theme,
            );
        }
    }

    fn render_text_field(
        frame: &mut Frame,
        area: Rect,
        state: &ConnectionSetupState,
        field: ConnectionField,
        mask: bool,
        theme: &ThemePalette,
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
            Style::default().fg(theme.semantic.text.secondary).bold()
        } else {
            Style::default().fg(theme.semantic.text.secondary)
        };
        let label_para = Paragraph::new(field.label()).style(label_style);
        frame.render_widget(label_para, chunks[0]);

        let display_value = if mask {
            "*".repeat(value.chars().count())
        } else {
            value.to_string()
        };

        let content_width = CONNECTION_INPUT_VISIBLE_WIDTH;

        let border_style = theme.modal_input_border_style(is_focused, error.is_some());

        let input_line = if is_focused {
            let input = state.focused_input().unwrap();
            let viewport = input.viewport_offset();
            let cursor = input.cursor();
            let char_count = display_value.chars().count();

            // same reservation logic as TextInputState::update_viewport
            let effective_width = if cursor >= char_count {
                content_width.saturating_sub(1)
            } else {
                content_width
            };

            let cursor_spans =
                text_cursor_spans(&display_value, cursor, viewport, effective_width, theme);

            // Calculate total display width of cursor spans (including block cursor)
            let used_width: usize = cursor_spans.iter().map(|s| s.content.chars().count()).sum();
            let padding = content_width.saturating_sub(used_width);

            let mut spans = vec![
                Span::styled("[", border_style),
                Span::styled(" ", Style::default().fg(theme.semantic.text.primary)),
            ];
            spans.extend(cursor_spans);
            if padding > 0 {
                spans.push(Span::raw(" ".repeat(padding)));
            }
            spans.push(Span::styled(
                " ",
                Style::default().fg(theme.semantic.text.primary),
            ));
            spans.push(Span::styled("]", border_style));
            Line::from(spans)
        } else {
            let truncated: String = display_value.chars().take(content_width).collect();
            let padding = content_width.saturating_sub(truncated.chars().count());
            bracketed_input(
                &format!("{}{}", truncated, " ".repeat(padding)),
                border_style,
                theme,
            )
        };

        let input_para = Paragraph::new(input_line);
        frame.render_widget(input_para, chunks[1]);

        if let Some(err) = error {
            let err_para = Paragraph::new(format!(" {err}"))
                .style(Style::default().fg(theme.semantic.status.error));
            frame.render_widget(err_para, chunks[2]);
        }
    }

    fn render_ssl_field(
        frame: &mut Frame,
        area: Rect,
        ssl_mode: SslMode,
        is_focused: bool,
        theme: &ThemePalette,
    ) {
        let chunks = Layout::horizontal([
            Constraint::Length(LABEL_WIDTH),
            Constraint::Length(INPUT_WIDTH),
            Constraint::Length(ERROR_WIDTH),
        ])
        .split(area);

        // Label: gray (like Explorer content), bold when focused
        let label_style = if is_focused {
            Style::default().fg(theme.semantic.text.secondary).bold()
        } else {
            Style::default().fg(theme.semantic.text.secondary)
        };
        let label_para = Paragraph::new("SSL Mode:").style(label_style);
        frame.render_widget(label_para, chunks[0]);

        // Value: white (emphasized), same width as text fields
        let content_width = CONNECTION_INPUT_VISIBLE_WIDTH;
        let ssl_mode_str = ssl_mode.to_string();
        let display_content = format!("{:<1$} ▼", ssl_mode_str, content_width - 2);

        let border_style = theme.modal_input_border_style(is_focused, false);

        let input_para = Paragraph::new(bracketed_input(&display_content, border_style, theme));
        frame.render_widget(input_para, chunks[1]);
    }

    fn render_database_type_field(
        frame: &mut Frame,
        area: Rect,
        database_type: DatabaseType,
        is_focused: bool,
        theme: &ThemePalette,
    ) {
        let chunks = Layout::horizontal([
            Constraint::Length(LABEL_WIDTH),
            Constraint::Length(INPUT_WIDTH),
            Constraint::Length(ERROR_WIDTH),
        ])
        .split(area);

        let label_style = if is_focused {
            Style::default().fg(theme.semantic.text.secondary).bold()
        } else {
            Style::default().fg(theme.semantic.text.secondary)
        };
        frame.render_widget(Paragraph::new("Type:").style(label_style), chunks[0]);

        let content_width = CONNECTION_INPUT_VISIBLE_WIDTH;
        let value = database_type.to_string();
        let display_content = format!("{:<1$} ▼", value, content_width - 2);
        let border_style = theme.modal_input_border_style(is_focused, false);

        frame.render_widget(
            Paragraph::new(bracketed_input(&display_content, border_style, theme)),
            chunks[1],
        );
    }

    fn render_database_type_dropdown(
        frame: &mut Frame,
        field_area: Rect,
        selected_index: usize,
        theme: &ThemePalette,
    ) {
        let chunks = Layout::horizontal([
            Constraint::Length(LABEL_WIDTH),
            Constraint::Length(INPUT_WIDTH),
            Constraint::Length(ERROR_WIDTH),
        ])
        .split(field_area);

        let dropdown_area = Rect {
            x: chunks[1].x,
            y: chunks[1].y + 1,
            width: INPUT_WIDTH,
            height: DB_TYPE_DROPDOWN_ITEM_COUNT as u16 + 2,
        };

        frame.render_widget(Clear, dropdown_area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border_style())
            .style(Style::default());
        frame.render_widget(block, dropdown_area);

        let inner = dropdown_area.inner(Margin::new(1, 1));
        for (i, variant) in DatabaseType::all().iter().enumerate() {
            let item_area = Rect {
                x: inner.x,
                y: inner.y + i as u16,
                width: inner.width,
                height: 1,
            };
            let style = if i == selected_index {
                theme.picker_selected_style()
            } else {
                Style::default().fg(theme.semantic.text.secondary)
            };
            frame.render_widget(Paragraph::new(variant.to_string()).style(style), item_area);
        }
    }

    fn render_dropdown(
        frame: &mut Frame,
        ssl_field_area: Rect,
        selected_index: usize,
        theme: &ThemePalette,
    ) {
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
            height: SSL_DROPDOWN_ITEM_COUNT as u16 + 2,
        };

        frame.render_widget(Clear, dropdown_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border_style())
            .style(Style::default());
        frame.render_widget(block, dropdown_area);

        let inner = dropdown_area.inner(Margin::new(1, 1));
        let variants = SslMode::all_variants();

        for (i, variant) in variants.iter().enumerate() {
            if i >= SSL_DROPDOWN_ITEM_COUNT {
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
                theme.picker_selected_style()
            } else {
                Style::default().fg(theme.semantic.text.secondary)
            };

            let item_para = Paragraph::new(variant.to_string()).style(item_style);
            frame.render_widget(item_para, item_area);
        }
    }
}
