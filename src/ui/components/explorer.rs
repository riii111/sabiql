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
        let has_cached_data = state.metadata.is_some() && !state.tables().is_empty();
        let is_focused = state.focused_pane == FocusedPane::Explorer;

        let title = match &state.metadata_state {
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
        let max_name_width = table_names
            .iter()
            .map(|n| char_count(n))
            .max()
            .unwrap_or(0);
        let h_offset = state.explorer_horizontal_offset;

        let items: Vec<ListItem> = if has_cached_data {
            table_names
                .iter()
                .map(|name| {
                    let displayed = truncate_with_offset(name, h_offset, content_width);
                    ListItem::new(displayed)
                })
                .collect()
        } else {
            match &state.metadata_state {
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
                .explorer_list_state
                .select(Some(state.explorer_selected));
        } else {
            state.explorer_list_state.select(None);
        }

        frame.render_stateful_widget(list, area, &mut state.explorer_list_state);

        // Render scrollbars
        if has_cached_data {
            let total_items = state.tables().len();
            let viewport_size = inner.height.saturating_sub(2) as usize; // Reserve for horizontal scrollbar

            if total_items > viewport_size {
                let scroll_offset = state.explorer_list_state.offset();

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

    mod truncate_with_offset {
        use super::*;

        #[test]
        fn ascii_basic() {
            assert_eq!(truncate_with_offset("abcdefgh", 0, 5), "abcde");
        }

        #[test]
        fn ascii_with_offset() {
            assert_eq!(truncate_with_offset("abcdefgh", 2, 4), "cdef");
        }

        #[test]
        fn offset_at_end() {
            assert_eq!(truncate_with_offset("abc", 3, 5), "");
        }

        #[test]
        fn offset_beyond_end() {
            assert_eq!(truncate_with_offset("abc", 10, 5), "");
        }

        #[test]
        fn japanese_no_panic() {
            let s = "日本語テスト";
            let result = truncate_with_offset(s, 0, 3);
            assert_eq!(result, "日本語");
        }

        #[test]
        fn japanese_with_offset() {
            let s = "日本語テスト";
            let result = truncate_with_offset(s, 2, 3);
            assert_eq!(result, "語テス");
        }

        #[test]
        fn mixed_ascii_unicode() {
            let s = "public.日本語_table";
            let result = truncate_with_offset(s, 0, 10);
            assert_eq!(result, "public.日本語");
        }

        #[test]
        fn emoji_handling() {
            let s = "🎉table🎊";
            let result = truncate_with_offset(s, 0, 6);
            assert_eq!(result, "🎉table");
        }
    }

    mod char_count {
        use super::*;

        #[test]
        fn ascii() {
            assert_eq!(char_count("hello"), 5);
        }

        #[test]
        fn japanese() {
            assert_eq!(char_count("日本語"), 3);
        }

        #[test]
        fn mixed() {
            assert_eq!(char_count("hello日本語"), 8);
        }
    }
}
