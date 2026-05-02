use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::theme::ThemePalette;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::ui_state::{
    HELP_MODAL_HEIGHT_PERCENT, HELP_MODAL_WIDTH_PERCENT, HelpViewportLayout,
    help_viewport_layout_for,
};
use crate::app::update::input::keybindings::{
    CELL_EDIT_KEYS, COMMAND_LINE_KEYS, COMMAND_PALETTE_ROWS, CONFIRM_DIALOG_KEYS,
    CONNECTION_ERROR_ROWS, CONNECTION_SELECTOR_ROWS, CONNECTION_SETUP_KEYS, ER_PICKER_ROWS,
    GLOBAL_KEYS, HELP_KEY_COLUMN_WIDTH, HELP_KEY_DESC_GAP, HELP_KEY_INDENT_WIDTH, HELP_ROWS,
    HISTORY_KEYS, INSPECTOR_DDL_KEYS, JSONB_DETAIL_ROWS, JSONB_EDIT_ROWS, JSONB_SEARCH_KEYS,
    KeyBinding, NAVIGATION_KEYS, OVERLAY_KEYS, QUERY_HISTORY_PICKER_ROWS, RESULT_ACTIVE_KEYS,
    SQL_MODAL_COMPARE_KEYS, SQL_MODAL_CONFIRMING_KEYS, SQL_MODAL_KEYS, SQL_MODAL_NORMAL_KEYS,
    SQL_MODAL_PLAN_KEYS, TABLE_PICKER_ROWS, help_content_width,
};

use crate::primitives::atoms::scroll_indicator::{
    HorizontalScrollParams, VerticalScrollParams, clamp_scroll_offset,
    render_horizontal_scroll_indicator, render_vertical_scroll_indicator_bar,
};
use crate::primitives::molecules::render_modal;

pub struct HelpOverlay;

impl HelpOverlay {
    pub fn render(frame: &mut Frame, state: &AppState, theme: &ThemePalette) {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(HELP_MODAL_WIDTH_PERCENT),
            Constraint::Percentage(HELP_MODAL_HEIGHT_PERCENT),
            " Help ",
            " ?/Esc Close ",
            theme,
        );

        let mut help_lines = vec![Self::section("Global Keys", theme)];
        Self::push_dedup(&mut help_lines, GLOBAL_KEYS, theme);

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Navigation", theme));
        for entry in NAVIGATION_KEYS {
            help_lines.push(Self::key_line(entry.key, entry.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Result History", theme));
        Self::push_dedup(&mut help_lines, HISTORY_KEYS, theme);

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Result Pane", theme));
        for kb in RESULT_ACTIVE_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Inspector Pane (DDL tab)", theme));
        for kb in INSPECTOR_DDL_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Cell Edit", theme));
        for kb in CELL_EDIT_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("SQL Editor (Normal)", theme));
        for kb in SQL_MODAL_NORMAL_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("SQL Editor (Insert)", theme));
        for kb in SQL_MODAL_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("SQL Editor (Plan)", theme));
        for kb in SQL_MODAL_PLAN_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("SQL Editor (Compare)", theme));
        for kb in SQL_MODAL_COMPARE_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("SQL Editor (Confirm)", theme));
        for kb in SQL_MODAL_CONFIRMING_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Overlays", theme));
        for kb in OVERLAY_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Command Line", theme));
        for kb in COMMAND_LINE_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Connection Setup", theme));
        for kb in CONNECTION_SETUP_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Connection Error", theme));
        for row in CONNECTION_ERROR_ROWS {
            help_lines.push(Self::key_line(row.key, row.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Connection Selector", theme));
        for row in CONNECTION_SELECTOR_ROWS {
            help_lines.push(Self::key_line(row.key, row.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("ER Diagram Picker", theme));
        for row in ER_PICKER_ROWS {
            help_lines.push(Self::key_line(row.key, row.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Query History Picker", theme));
        for row in QUERY_HISTORY_PICKER_ROWS {
            help_lines.push(Self::key_line(row.key, row.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Table Picker", theme));
        for row in TABLE_PICKER_ROWS {
            help_lines.push(Self::key_line(row.key, row.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Command Palette", theme));
        for row in COMMAND_PALETTE_ROWS {
            help_lines.push(Self::key_line(row.key, row.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Help Overlay", theme));
        for row in HELP_ROWS {
            help_lines.push(Self::key_line(row.key, row.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Confirm Dialog", theme));
        for kb in CONFIRM_DIALOG_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("JSONB Detail", theme));
        for row in JSONB_DETAIL_ROWS {
            help_lines.push(Self::key_line(row.key, row.description, theme));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("JSONB Edit", theme));
        for row in JSONB_EDIT_ROWS {
            help_lines.push(Self::key_line(row.key, row.description, theme));
        }

        help_lines.push(Line::raw(""));
        help_lines.push(Self::section("JSONB Search", theme));
        for kb in JSONB_SEARCH_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description, theme));
        }

        let total_lines = help_lines.len();
        let content_width = help_content_width();
        let viewport = help_viewport_layout_for(
            inner.height as usize,
            inner.width as usize,
            total_lines,
            content_width,
        );
        let content_area = Self::content_area(inner, viewport);
        let viewport_height = content_area.height as usize;
        let scroll_offset =
            clamp_scroll_offset(state.ui.help_scroll_offset, viewport_height, total_lines);
        let viewport_width = content_area.width as usize;
        let horizontal_offset = clamp_scroll_offset(
            state.ui.help_horizontal_offset,
            viewport_width,
            content_width,
        );

        let help = Paragraph::new(help_lines)
            .style(Style::default())
            .scroll((scroll_offset as u16, horizontal_offset as u16));

        frame.render_widget(help, content_area);

        if viewport.has_horizontal_scrollbar {
            render_horizontal_scroll_indicator(
                frame,
                inner,
                HorizontalScrollParams {
                    position: horizontal_offset,
                    viewport_size: viewport_width,
                    total_items: content_width,
                },
                theme,
            );
        }

        if viewport.has_vertical_scrollbar {
            render_vertical_scroll_indicator_bar(
                frame,
                inner,
                VerticalScrollParams {
                    position: scroll_offset,
                    viewport_size: viewport_height,
                    total_items: total_lines,
                    has_horizontal_scrollbar: viewport.has_horizontal_scrollbar,
                },
                theme,
            );
        }
    }

    fn content_area(inner: Rect, viewport: HelpViewportLayout) -> Rect {
        Rect {
            height: viewport.visible_rows as u16,
            width: viewport.visible_columns as u16,
            ..inner
        }
    }

    fn section(title: &str, theme: &ThemePalette) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                "▸ ",
                Style::default().fg(theme.component.navigation.section_header),
            ),
            Span::styled(
                title.to_string(),
                Style::default()
                    .fg(theme.component.navigation.section_header)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    }

    fn push_dedup(lines: &mut Vec<Line<'static>>, bindings: &[KeyBinding], theme: &ThemePalette) {
        let mut i = 0;
        while i < bindings.len() {
            if i + 1 < bindings.len() && bindings[i].key == bindings[i + 1].key {
                let toggle_desc = format!("Toggle {}", bindings[i].desc_short);
                lines.push(Self::key_line(bindings[i].key, &toggle_desc, theme));
                i += 2;
            } else {
                lines.push(Self::key_line(
                    bindings[i].key,
                    bindings[i].description,
                    theme,
                ));
                i += 1;
            }
        }
    }

    fn key_line(key: &str, desc: &str, theme: &ThemePalette) -> Line<'static> {
        let key_width = UnicodeWidthStr::width(key);
        let padding = if key_width > HELP_KEY_COLUMN_WIDTH {
            HELP_KEY_DESC_GAP
        } else {
            HELP_KEY_COLUMN_WIDTH.saturating_sub(key_width)
        };

        Line::from(vec![
            Span::styled(
                format!(
                    "{}{key}{}",
                    " ".repeat(HELP_KEY_INDENT_WIDTH),
                    " ".repeat(padding)
                ),
                Style::default()
                    .fg(theme.semantic.text.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                desc.to_string(),
                Style::default().fg(theme.semantic.text.secondary),
            ),
        ])
    }
}
