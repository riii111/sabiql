use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::focused_pane::FocusedPane;
use crate::app::state::AppState;
use crate::domain::MetadataState;

pub struct Explorer;

impl Explorer {
    pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState) {
        let has_cached_data = state.cache.metadata.is_some() && !state.tables().is_empty();
        let is_focused = state.ui.focused_pane == FocusedPane::Explorer;

        let title = match &state.cache.state {
            MetadataState::Loading => " [1] Explorer [Loading...] ".to_string(),
            MetadataState::Error(_) if has_cached_data => {
                format!(" [1] Explorer [{} tables - Stale] ", state.tables().len())
            }
            MetadataState::Error(_) => " [1] Explorer [Error] ".to_string(),
            MetadataState::Loaded => {
                let count = state.tables().len();
                format!(" [1] Explorer [{} tables] ", count)
            }
            MetadataState::NotLoaded => " [1] Explorer ".to_string(),
        };

        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        // Calculate inner area first
        let inner = block.inner(area);

        // Calculate content width (reserve space for highlight symbol and scrollbar)
        let highlight_symbol_width: u16 = 2; // "> "
        let scrollbar_reserved: u16 = 1;
        let content_width = inner
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
                    vec![ListItem::new("Loading metadata...")]
                }
                MetadataState::Error(e) => {
                    vec![ListItem::new(format!("Error: {}", e))]
                }
                MetadataState::Loaded => {
                    vec![ListItem::new("No tables found")]
                }
                MetadataState::NotLoaded => {
                    vec![ListItem::new("Press 'r' to load metadata")]
                }
            }
        };

        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        if has_cached_data {
            state
                .ui
                .explorer_list_state
                .select(Some(state.ui.explorer_selected));
        } else {
            state.ui.explorer_list_state.select(None);
        }

        frame.render_stateful_widget(list, area, &mut state.ui.explorer_list_state);

        // Render scrollbars
        if has_cached_data {
            let total_items = state.tables().len();
            let viewport_size = inner.height.saturating_sub(2) as usize; // Reserve for horizontal scrollbar

            if total_items > viewport_size {
                let scroll_offset = state.ui.explorer_list_state.offset();

                use super::scroll_indicator::{
                    VerticalScrollParams, render_vertical_scroll_indicator_bar,
                };
                render_vertical_scroll_indicator_bar(
                    frame,
                    inner,
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
                    inner,
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
