use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::focused_pane::FocusedPane;
use crate::app::state::AppState;
use crate::domain::MetadataState;

pub struct Explorer;

impl Explorer {
    pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState) {
        let is_error = matches!(state.cache.state, MetadataState::Error(_));
        // Don't show stale tables when in error state - keeps Enter behavior consistent
        let has_cached_data =
            !is_error && state.cache.metadata.is_some() && !state.tables().is_empty();
        let is_focused = state.ui.focused_pane == FocusedPane::Explorer;

        let title = " [1] Explorer ";

        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let [connections_area, separator_area, tables_area] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(inner);

        Self::render_connections_section(frame, connections_area, state);
        frame.render_widget(Paragraph::new(""), separator_area);
        Self::render_tables_section(frame, tables_area, state, has_cached_data);
    }

    fn render_connections_section(frame: &mut Frame, area: Rect, state: &AppState) {
        let header_style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);
        let content_style = Style::default().fg(Color::Gray);

        let connection_display = if let Some(ref name) = state.runtime.active_connection_name {
            format!(" Current: {}", name)
        } else {
            " Current: (not connected)".to_string()
        };

        let lines = vec![
            Line::from(Span::styled("Connections", header_style)),
            Line::from(Span::styled(connection_display, content_style)),
        ];

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_tables_section(
        frame: &mut Frame,
        area: Rect,
        state: &mut AppState,
        has_cached_data: bool,
    ) {
        let tables_header = match &state.cache.state {
            MetadataState::Loaded => format!("Tables [{}]", state.tables().len()),
            _ => "Tables".to_string(),
        };

        let header_style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);

        let [header_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(tables_header, header_style))),
            header_area,
        );

        let highlight_symbol_width: u16 = 2; // "> "
        let scrollbar_reserved: u16 = 1;
        let content_width = list_area
            .width
            .saturating_sub(highlight_symbol_width + scrollbar_reserved)
            as usize;

        let table_names: Vec<String> = if has_cached_data {
            state.tables().iter().map(|t| t.qualified_name()).collect()
        } else {
            Vec::new()
        };
        let max_name_width = table_names.iter().map(|n| char_count(n)).max().unwrap_or(0);
        let h_offset = state.ui.explorer_horizontal_offset;

        let items: Vec<ListItem> = if has_cached_data {
            table_names
                .iter()
                .map(|name| {
                    let displayed = truncate_with_offset(name, h_offset, content_width);
                    ListItem::new(displayed)
                })
                .collect()
        } else {
            match &state.cache.state {
                MetadataState::Loading => {
                    vec![ListItem::new(" Loading metadata...")]
                }
                MetadataState::Error(_) => {
                    vec![
                        ListItem::new(" Metadata load failed"),
                        ListItem::new(" (r: retry, Enter: details)"),
                    ]
                }
                MetadataState::Loaded => {
                    vec![ListItem::new(" No tables found")]
                }
                MetadataState::NotLoaded => {
                    vec![ListItem::new(" Press 'r' to load metadata")]
                }
            }
        };

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, list_area, &mut state.ui.explorer_list_state);

        // Render scrollbars
        if has_cached_data {
            let total_items = state.tables().len();
            let viewport_size = list_area.height.saturating_sub(1) as usize; // Reserve for horizontal scrollbar

            if total_items > viewport_size {
                let scroll_offset = state.ui.explorer_list_state.offset();

                use super::scroll_indicator::{
                    VerticalScrollParams, render_vertical_scroll_indicator_bar,
                };
                render_vertical_scroll_indicator_bar(
                    frame,
                    list_area,
                    VerticalScrollParams {
                        position: scroll_offset,
                        viewport_size,
                        total_items,
                    },
                );
            }

            // Render horizontal scrollbar
            if max_name_width > content_width {
                use super::scroll_indicator::{
                    HorizontalScrollParams, render_horizontal_scroll_indicator,
                };
                render_horizontal_scroll_indicator(
                    frame,
                    list_area,
                    HorizontalScrollParams {
                        position: h_offset,
                        viewport_size: content_width,
                        total_items: max_name_width,
                    },
                );
            }
        }
    }
}

fn truncate_with_offset(s: &str, offset: usize, max_width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let total_len = chars.len();

    if offset >= total_len {
        return String::new();
    }

    let end = (offset + max_width).min(total_len);
    chars[offset..end].iter().collect()
}

/// Returns character count (not byte length)
fn char_count(s: &str) -> usize {
    s.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod truncate_with_offset {
        use super::*;

        #[rstest]
        #[case("abcdefgh", 0, 5, "abcde")]
        #[case("abcdefgh", 2, 4, "cdef")]
        #[case("abc", 3, 5, "")]
        #[case("abc", 10, 5, "")]
        fn ascii_input_returns_expected_substring(
            #[case] input: &str,
            #[case] offset: usize,
            #[case] max_width: usize,
            #[case] expected: &str,
        ) {
            let result = truncate_with_offset(input, offset, max_width);

            assert_eq!(result, expected);
        }

        #[rstest]
        #[case("日本語テスト", 0, 3, "日本語")]
        #[case("日本語テスト", 2, 3, "語テス")]
        #[case("public.日本語_table", 0, 10, "public.日本語")]
        #[case("🎉table🎊", 0, 6, "🎉table")]
        fn unicode_input_returns_expected_substring(
            #[case] input: &str,
            #[case] offset: usize,
            #[case] max_width: usize,
            #[case] expected: &str,
        ) {
            let result = truncate_with_offset(input, offset, max_width);

            assert_eq!(result, expected);
        }
    }

    mod char_count {
        use super::*;

        #[rstest]
        #[case("hello", 5)]
        #[case("日本語", 3)]
        #[case("hello日本語", 8)]
        #[case("", 0)]
        fn input_returns_character_count(#[case] input: &str, #[case] expected: usize) {
            let result = char_count(input);

            assert_eq!(result, expected);
        }
    }
}
