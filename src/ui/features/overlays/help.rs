use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::theme::ThemePalette;

use crate::app::catalog::{HelpDocument, HelpRow};
use crate::app::model::app_state::AppState;
use crate::app::model::shared::ui_state::{
    HELP_MODAL_HEIGHT_PERCENT, HELP_MODAL_WIDTH_PERCENT, HelpViewportLayout,
    help_viewport_layout_for,
};
use crate::app::update::input::keybindings::{HELP_KEY_DESC_GAP, HELP_KEY_INDENT_WIDTH};

use crate::primitives::atoms::scroll_indicator::{
    HorizontalScrollParams, VerticalScrollParams, clamp_scroll_offset,
    render_horizontal_scroll_indicator, render_vertical_scroll_indicator_bar,
};
use crate::primitives::atoms::text_cursor_spans;
use crate::primitives::molecules::{FooterHintBar, render_modal};

pub struct HelpOverlay;

impl HelpOverlay {
    pub fn render(frame: &mut Frame, state: &AppState, theme: &ThemePalette) {
        let document = HelpDocument::from_state(state);
        let footer = FooterHintBar::new([
            ("type", "Filter"),
            ("Backspace", "Edit"),
            ("Esc", "Close"),
            ("?", "Close"),
        ]);
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(HELP_MODAL_WIDTH_PERCENT),
            Constraint::Percentage(HELP_MODAL_HEIGHT_PERCENT),
            " Help ",
            footer,
            theme,
        );

        let help_lines = Self::document_lines(&document, theme);
        let total_lines = document.line_count();
        let content_width = document.content_width();
        let viewport = help_viewport_layout_for(
            inner.height as usize,
            inner.width as usize,
            total_lines,
            content_width,
        );
        let content_area = Self::content_area(inner, viewport);
        let viewport_height = content_area.height as usize;
        let scroll_offset =
            clamp_scroll_offset(state.ui.help.scroll_offset(), viewport_height, total_lines);
        let viewport_width = content_area.width as usize;
        let horizontal_offset = clamp_scroll_offset(
            state.ui.help.horizontal_offset(),
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

    fn document_lines(document: &HelpDocument, theme: &ThemePalette) -> Vec<Line<'static>> {
        let key_column_width = document.key_column_width();
        let mut lines = vec![Self::filter_line(document, theme), Line::raw("")];
        for (index, section) in document.sections().iter().enumerate() {
            if index > 0 {
                lines.push(Line::raw(""));
            }
            lines.push(Self::section(section.title(), theme));
            for row in section.rows() {
                lines.push(Self::row_line(row, key_column_width, theme));
            }
        }
        lines
    }

    fn filter_line(document: &HelpDocument, theme: &ThemePalette) -> Line<'static> {
        let mut spans = vec![Span::styled(
            "Filter: ",
            Style::default().fg(theme.semantic.text.secondary),
        )];
        spans.extend(text_cursor_spans(
            document.filter(),
            document.filter_cursor(),
            0,
            usize::MAX,
            theme,
        ));
        Line::from(spans)
    }

    fn row_line(row: &HelpRow, key_column_width: usize, theme: &ThemePalette) -> Line<'static> {
        Self::key_line(row.key(), row.description(), key_column_width, theme)
    }

    fn key_line(
        key: &str,
        desc: &str,
        key_column_width: usize,
        theme: &ThemePalette,
    ) -> Line<'static> {
        let key_width = UnicodeWidthStr::width(key);
        let padding = key_column_width
            .saturating_sub(key_width)
            .saturating_add(HELP_KEY_DESC_GAP);

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
