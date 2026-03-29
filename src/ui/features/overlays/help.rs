use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::ui::theme::Theme;

use crate::app::model::app_state::AppState;
use crate::app::update::input::keybindings::{
    CELL_EDIT_KEYS, COMMAND_LINE_KEYS, COMMAND_PALETTE_ROWS, CONFIRM_DIALOG_KEYS,
    CONNECTION_ERROR_ROWS, CONNECTION_SELECTOR_ROWS, CONNECTION_SETUP_KEYS, ER_PICKER_ROWS,
    GLOBAL_KEYS, HELP_ROWS, HISTORY_KEYS, INSPECTOR_DDL_KEYS, JSONB_DETAIL_KEYS, JSONB_EDIT_KEYS,
    JSONB_SEARCH_KEYS, KeyBinding, NAVIGATION_KEYS, OVERLAY_KEYS, QUERY_HISTORY_PICKER_ROWS,
    RESULT_ACTIVE_KEYS, SQL_MODAL_COMPARE_KEYS, SQL_MODAL_CONFIRMING_KEYS, SQL_MODAL_KEYS,
    SQL_MODAL_NORMAL_KEYS, SQL_MODAL_PLAN_KEYS, TABLE_PICKER_ROWS,
};

use crate::ui::primitives::atoms::scroll_indicator::{
    VerticalScrollParams, clamp_scroll_offset, render_vertical_scroll_indicator_bar,
};
use crate::ui::primitives::molecules::render_modal;

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
        Self::push_dedup(&mut help_lines, GLOBAL_KEYS);

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Navigation"));
        for entry in NAVIGATION_KEYS {
            help_lines.push(Self::key_line(entry.key, entry.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Result History"));
        Self::push_dedup(&mut help_lines, HISTORY_KEYS);

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Result Pane"));
        for kb in RESULT_ACTIVE_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Inspector Pane (DDL tab)"));
        for kb in INSPECTOR_DDL_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Cell Edit"));
        for kb in CELL_EDIT_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("SQL Editor (Normal)"));
        for kb in SQL_MODAL_NORMAL_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("SQL Editor (Insert)"));
        for kb in SQL_MODAL_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("SQL Editor (Plan)"));
        for kb in SQL_MODAL_PLAN_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("SQL Editor (Compare)"));
        for kb in SQL_MODAL_COMPARE_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("SQL Editor (Confirm)"));
        for kb in SQL_MODAL_CONFIRMING_KEYS {
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
        for row in CONNECTION_ERROR_ROWS {
            help_lines.push(Self::key_line(row.key, row.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Connection Selector"));
        for row in CONNECTION_SELECTOR_ROWS {
            help_lines.push(Self::key_line(row.key, row.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("ER Diagram Picker"));
        for row in ER_PICKER_ROWS {
            help_lines.push(Self::key_line(row.key, row.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Query History Picker"));
        for row in QUERY_HISTORY_PICKER_ROWS {
            help_lines.push(Self::key_line(row.key, row.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Table Picker"));
        for row in TABLE_PICKER_ROWS {
            help_lines.push(Self::key_line(row.key, row.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Command Palette"));
        for row in COMMAND_PALETTE_ROWS {
            help_lines.push(Self::key_line(row.key, row.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Help Overlay"));
        for row in HELP_ROWS {
            help_lines.push(Self::key_line(row.key, row.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("Confirm Dialog"));
        for kb in CONFIRM_DIALOG_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("JSONB Detail"));
        for kb in JSONB_DETAIL_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Self::section("JSONB Edit"));
        for kb in JSONB_EDIT_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        help_lines.push(Line::raw(""));
        help_lines.push(Self::section("JSONB Search"));
        for kb in JSONB_SEARCH_KEYS {
            help_lines.push(Self::key_line(kb.key, kb.description));
        }

        let total_lines = help_lines.len();
        let viewport_height = inner.height as usize;
        let scroll_offset =
            clamp_scroll_offset(state.ui.help_scroll_offset, viewport_height, total_lines);

        let help = Paragraph::new(help_lines)
            .wrap(Wrap { trim: false })
            .style(Style::default())
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
            Span::styled("▸ ", Style::default().fg(Theme::SECTION_HEADER)),
            Span::styled(
                title.to_string(),
                Style::default()
                    .fg(Theme::SECTION_HEADER)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    }

    fn push_dedup(lines: &mut Vec<Line<'static>>, bindings: &[KeyBinding]) {
        let mut i = 0;
        while i < bindings.len() {
            if i + 1 < bindings.len() && bindings[i].key == bindings[i + 1].key {
                let toggle_desc = format!("Toggle {}", bindings[i].desc_short);
                lines.push(Self::key_line(bindings[i].key, &toggle_desc));
                i += 2;
            } else {
                lines.push(Self::key_line(bindings[i].key, bindings[i].description));
                i += 1;
            }
        }
    }

    fn key_line(key: &str, desc: &str) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                format!("  {key:<20}"),
                Style::default()
                    .fg(Theme::TEXT_ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(desc.to_string(), Style::default().fg(Theme::TEXT_SECONDARY)),
        ])
    }
}
