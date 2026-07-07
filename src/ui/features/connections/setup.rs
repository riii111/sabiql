use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::model::app_state::AppState;
use crate::app::model::connection::setup::{
    CONNECTION_INPUT_VISIBLE_WIDTH, CONNECTION_INPUT_WIDTH, ConnectionField, ConnectionSetupState,
};
use crate::app::policy::mask_password;
use crate::app::services::AppServices;
use crate::app::update::input::keybindings::{connection_setup, connection_setup_save};
use crate::domain::connection::{ConnectionId, ConnectionProfile, SslMode};
use crate::primitives::atoms::text_cursor_spans;
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::primitives::utils::text_utils::{take_within_width, truncate_to_width_with};
use crate::theme::ThemePalette;

const LABEL_WIDTH: u16 = 12;
const INPUT_WIDTH: u16 = CONNECTION_INPUT_WIDTH;
const ERROR_WIDTH: u16 = 12;
const FIELD_HEIGHT: u16 = 1;
const DROPDOWN_ITEM_COUNT: usize = 6;
const MODAL_HORIZONTAL_CHROME: u16 = 6;
const MODAL_VERTICAL_CHROME: u16 = 4;
const NON_PREVIEW_CONTENT_HEIGHT: u16 = 9;
const MIN_PREVIEW_LINES: usize = 2;

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
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        services: &AppServices,
        theme: &ThemePalette,
    ) {
        let form_state = &state.connection_setup;

        let (title, submit_desc) = if form_state.is_edit_mode() {
            (" Edit Connection ", "Save")
        } else {
            (" New Connection ", "Connect")
        };
        let submit_hints = Self::submit_hints(state, form_state, submit_desc);
        let mut footer_hints = vec![connection_setup::TAB_NAV.as_hint()];
        footer_hints.extend(submit_hints);
        footer_hints.push(("Esc", "Cancel"));
        let footer = FooterHintBar::new(footer_hints);

        let modal_width = LABEL_WIDTH + INPUT_WIDTH + ERROR_WIDTH + 8;
        let preview = preview_text(form_state, services);
        let preview_width = modal_width.saturating_sub(MODAL_HORIZONTAL_CHROME) as usize;
        let max_preview_lines = frame
            .area()
            .height
            .saturating_sub(MODAL_VERTICAL_CHROME + NON_PREVIEW_CONTENT_HEIGHT)
            .max(MIN_PREVIEW_LINES as u16) as usize;
        let preview_lines = preview_lines(&preview, preview_width, max_preview_lines);
        let modal_height =
            MODAL_VERTICAL_CHROME + NON_PREVIEW_CONTENT_HEIGHT + preview_lines.len() as u16;
        let (_, modal_inner) = render_modal(
            frame,
            Constraint::Length(modal_width),
            Constraint::Length(modal_height),
            title,
            footer,
            theme,
        );

        let inner = modal_inner.inner(Margin::new(2, 1));
        let chunks = Layout::vertical([
            Constraint::Length(FIELD_HEIGHT),               // Name
            Constraint::Length(FIELD_HEIGHT),               // Host
            Constraint::Length(FIELD_HEIGHT),               // Port
            Constraint::Length(FIELD_HEIGHT),               // Database
            Constraint::Length(FIELD_HEIGHT),               // User
            Constraint::Length(FIELD_HEIGHT),               // Password
            Constraint::Length(FIELD_HEIGHT),               // SslMode
            Constraint::Length(1),                          // spacer
            Constraint::Length(preview_lines.len() as u16), // DSN preview
            Constraint::Length(1),                          // notice
        ])
        .split(inner);

        Self::render_text_field(
            frame,
            chunks[0],
            form_state,
            ConnectionField::Name,
            false,
            theme,
        );
        Self::render_text_field(
            frame,
            chunks[1],
            form_state,
            ConnectionField::Host,
            false,
            theme,
        );
        Self::render_text_field(
            frame,
            chunks[2],
            form_state,
            ConnectionField::Port,
            false,
            theme,
        );
        Self::render_text_field(
            frame,
            chunks[3],
            form_state,
            ConnectionField::Database,
            false,
            theme,
        );
        Self::render_text_field(
            frame,
            chunks[4],
            form_state,
            ConnectionField::User,
            false,
            theme,
        );
        Self::render_text_field(
            frame,
            chunks[5],
            form_state,
            ConnectionField::Password,
            true,
            theme,
        );
        Self::render_ssl_field(
            frame,
            chunks[6],
            form_state.ssl_mode,
            form_state.focused_field == ConnectionField::SslMode,
            theme,
        );
        Self::render_dsn_preview(frame, chunks[8], &preview_lines, theme);

        let notice = "Note: Connection info is stored locally in plain text";
        let notice_para =
            Paragraph::new(notice).style(Style::default().fg(theme.component.feedback.note_text));
        frame.render_widget(notice_para, chunks[9]);

        if form_state.ssl_dropdown.is_open {
            Self::render_dropdown(
                frame,
                chunks[6],
                form_state.ssl_dropdown.selected_index,
                theme,
            );
        }
    }

    fn submit_hints(
        state: &AppState,
        form_state: &ConnectionSetupState,
        submit_desc: &'static str,
    ) -> Vec<(&'static str, &'static str)> {
        if form_state.focused_field == ConnectionField::SslMode {
            vec![
                connection_setup::ENTER_DROPDOWN.as_hint(),
                (connection_setup::SAVE.key_short, submit_desc),
            ]
        } else {
            vec![(
                connection_setup_save(state.settings.saved_keymap_preset()).key_short,
                submit_desc,
            )]
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

        let placeholder = field.placeholder();
        let show_placeholder = value.is_empty() && !placeholder.is_empty();

        let input_line = if is_focused {
            let input = state.focused_input().unwrap();
            let viewport = input.viewport_offset();
            let cursor = input.cursor();
            let char_count = display_value.chars().count();

            let effective_width = if cursor >= char_count {
                content_width.saturating_sub(1)
            } else {
                content_width
            };

            let cursor_spans = if show_placeholder && !mask {
                focused_placeholder_spans(placeholder, effective_width, theme)
            } else {
                text_cursor_spans(&display_value, cursor, viewport, effective_width, theme)
            };

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
            let display = if show_placeholder {
                placeholder.to_string()
            } else {
                display_value
            };
            let truncated: String = display.chars().take(content_width).collect();
            let padding = content_width.saturating_sub(truncated.chars().count());
            let content = format!("{}{}", truncated, " ".repeat(padding));
            if show_placeholder {
                Line::from(vec![
                    Span::styled("[", border_style),
                    Span::styled(
                        format!(" {content} "),
                        Style::default().fg(theme.semantic.text.placeholder),
                    ),
                    Span::styled("]", border_style),
                ])
            } else {
                bracketed_input(&content, border_style, theme)
            }
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

    fn render_dsn_preview(
        frame: &mut Frame,
        area: Rect,
        preview_lines: &[String],
        theme: &ThemePalette,
    ) {
        let lines = preview_lines
            .iter()
            .map(|line| {
                Line::from(Span::styled(
                    line.as_str(),
                    Style::default().fg(theme.semantic.text.secondary),
                ))
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), area);
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
            height: DROPDOWN_ITEM_COUNT as u16 + 2,
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
                theme.picker_selected_style()
            } else {
                Style::default().fg(theme.semantic.text.secondary)
            };

            let item_para = Paragraph::new(variant.to_string()).style(item_style);
            frame.render_widget(item_para, item_area);
        }
    }
}

fn focused_placeholder_spans(
    placeholder: &str,
    effective_width: usize,
    theme: &ThemePalette,
) -> Vec<Span<'static>> {
    if effective_width == 0 {
        return vec![];
    }

    let placeholder_text = take_within_width(placeholder, effective_width.saturating_sub(1));
    vec![
        Span::styled(" ", theme.block_cursor_style()),
        Span::styled(
            placeholder_text,
            Style::default().fg(theme.semantic.text.placeholder),
        ),
    ]
}

fn preview_profile(state: &ConnectionSetupState) -> ConnectionProfile {
    let port = state.port.content().trim().parse().unwrap_or(5432);
    ConnectionProfile::with_id(
        ConnectionId::from_string("preview"),
        "preview",
        state.host.content().trim(),
        port,
        state.database.content(),
        state.user.content().trim(),
        state.password.content(),
        state.ssl_mode,
    )
    .expect("static preview connection name is valid")
}

fn preview_text(form_state: &ConnectionSetupState, services: &AppServices) -> String {
    let profile = preview_profile(form_state);
    mask_password(&services.dsn_builder.build_dsn(&profile))
}

fn preview_lines(dsn: &str, width: usize, max_lines: usize) -> Vec<String> {
    const FIRST_PREFIX: &str = "→ ";
    const CONTINUATION_PREFIX: &str = "  ";

    if width == 0 || max_lines == 0 {
        return vec![];
    }

    let mut remaining = dsn;
    let mut lines = Vec::new();

    for index in 0..max_lines {
        let prefix = if index == 0 {
            preview_prefix(FIRST_PREFIX, width)
        } else {
            preview_prefix(CONTINUATION_PREFIX, width)
        };
        let available = width.saturating_sub(UnicodeWidthStr::width(prefix));

        if index + 1 == max_lines {
            let last_segment = truncate_to_width_with(remaining, available, "…");
            lines.push(format!("{prefix}{last_segment}"));
            break;
        }

        let segment = take_within_width(remaining, available);
        remaining = &remaining[segment.len()..];
        lines.push(format!("{prefix}{segment}"));

        if remaining.is_empty() {
            break;
        }
    }

    lines
}

fn preview_prefix(prefix: &'static str, width: usize) -> &'static str {
    if UnicodeWidthStr::width(prefix) >= width {
        ""
    } else {
        prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::settings::KeymapPreset;

    #[test]
    fn submit_hints_include_toggle_and_save_on_ssl_field() {
        let state = AppState::new("test".to_string());
        let form_state = ConnectionSetupState {
            focused_field: ConnectionField::SslMode,
            ..Default::default()
        };

        assert_eq!(
            ConnectionSetup::submit_hints(&state, &form_state, "Connect"),
            vec![("Enter", "Toggle"), ("^S", "Connect")]
        );
    }

    #[test]
    fn submit_hints_use_preset_save_key_off_ssl_field() {
        let mut state = AppState::new("test".to_string());
        state.settings.load_keymap_preset(KeymapPreset::Ide);
        let form_state = ConnectionSetupState::default();

        assert_eq!(
            ConnectionSetup::submit_hints(&state, &form_state, "Connect"),
            vec![("Enter", "Connect")]
        );
    }

    #[test]
    fn submit_hints_use_short_default_save_key_off_ssl_field() {
        let state = AppState::new("test".to_string());
        let form_state = ConnectionSetupState::default();

        assert_eq!(
            ConnectionSetup::submit_hints(&state, &form_state, "Connect"),
            vec![("^S", "Connect")]
        );
    }

    #[test]
    fn preview_profile_trims_host_and_user() {
        let mut form_state = ConnectionSetupState::default();
        form_state.host.set_content("  localhost  ".to_string());
        form_state.user.set_content("  postgres  ".to_string());
        form_state.password.set_content("  pass  ".to_string());

        let profile = preview_profile(&form_state);

        assert_eq!(profile.host, "localhost");
        assert_eq!(profile.username, "postgres");
        assert_eq!(profile.password, "  pass  ");
    }

    #[test]
    fn preview_lines_use_two_rows_with_ellipsis() {
        assert_eq!(
            preview_lines("host='localhost' port='5432'", 12, 2),
            vec!["→ host='loca".to_string(), "  lhost' po…".to_string()]
        );
    }

    #[test]
    fn preview_lines_respect_display_width() {
        let lines = preview_lines("dbname='日本語db' sslmode='prefer'", 12, 2);

        assert_eq!(
            lines,
            vec!["→ dbname='日".to_string(), "  本語db' s…".to_string()]
        );
        assert!(
            lines
                .iter()
                .all(|line| UnicodeWidthStr::width(line.as_str()) <= 12)
        );
    }

    #[test]
    fn preview_lines_expand_beyond_two_rows_before_truncating() {
        assert_eq!(
            preview_lines("abcdefghijklmnopqrstuvwx", 8, 4),
            vec![
                "→ abcdef".to_string(),
                "  ghijkl".to_string(),
                "  mnopqr".to_string(),
                "  stuvwx".to_string(),
            ]
        );
    }

    #[test]
    fn preview_lines_truncate_on_last_available_row() {
        assert_eq!(
            preview_lines("abcdefghijklmnopqrstuvwxyz", 8, 4),
            vec![
                "→ abcdef".to_string(),
                "  ghijkl".to_string(),
                "  mnopqr".to_string(),
                "  stuvw…".to_string(),
            ]
        );
    }
}
