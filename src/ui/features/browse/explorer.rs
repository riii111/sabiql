use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{List, ListItem, ListState};
use unicode_width::UnicodeWidthChar;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::ui_state::{
    explorer_content_width_from_inner_width, scroll_max_offset, text_display_width,
};
use crate::domain::MetadataState;
use crate::ui::theme::ThemePalette;

use crate::ui::primitives::atoms::panel_block;

pub struct Explorer;

impl Explorer {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) {
        let is_focused = state.ui.focused_pane == FocusedPane::Explorer;
        let block = panel_block(" [1] Explorer ", is_focused, theme);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let is_error = matches!(state.session.metadata_state(), MetadataState::Error(_));
        let has_cached_data =
            !is_error && state.session.metadata().is_some() && !state.tables().is_empty();
        Self::render_tables_section(frame, inner, state, has_cached_data, theme);
    }

    fn render_tables_section(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        has_cached_data: bool,
        theme: &ThemePalette,
    ) {
        let content_width = explorer_content_width_from_inner_width(area.width);

        let table_names: Vec<String> = if has_cached_data {
            state.tables().iter().map(|t| t.qualified_name()).collect()
        } else {
            Vec::new()
        };
        let max_name_width = table_names
            .iter()
            .map(|name| text_display_width(name))
            .max()
            .unwrap_or(0);
        let max_offset = scroll_max_offset(max_name_width, content_width);
        let h_offset = state.ui.explorer_horizontal_offset.min(max_offset);

        let items: Vec<ListItem> = if has_cached_data {
            table_names
                .iter()
                .map(|name| {
                    let displayed = truncate_with_offset(name, h_offset, content_width);
                    ListItem::new(displayed)
                })
                .collect()
        } else {
            match &state.session.metadata_state() {
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
            .style(Style::default().fg(theme.text_primary))
            .highlight_style(
                Style::default()
                    .fg(theme.text_accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let selected = if has_cached_data {
            Some(state.ui.explorer_selected)
        } else {
            None
        };
        let mut list_state = ListState::default()
            .with_selected(selected)
            .with_offset(state.ui.explorer_scroll_offset);
        frame.render_stateful_widget(list, area, &mut list_state);

        // Render scrollbars
        if has_cached_data {
            let total_items = state.tables().len();
            let viewport_size = area.height.saturating_sub(1) as usize; // Reserve for horizontal scrollbar

            if total_items > viewport_size {
                let scroll_offset = state.ui.explorer_scroll_offset;

                use crate::ui::primitives::atoms::scroll_indicator::{
                    VerticalScrollParams, render_vertical_scroll_indicator_bar,
                };
                render_vertical_scroll_indicator_bar(
                    frame,
                    area,
                    VerticalScrollParams {
                        position: scroll_offset,
                        viewport_size,
                        total_items,
                    },
                    theme,
                );
            }

            // Render horizontal scrollbar
            if max_name_width > content_width {
                use crate::ui::primitives::atoms::scroll_indicator::{
                    HorizontalScrollParams, render_horizontal_scroll_indicator,
                };
                render_horizontal_scroll_indicator(
                    frame,
                    area,
                    HorizontalScrollParams {
                        position: h_offset,
                        viewport_size: content_width,
                        total_items: max_name_width,
                    },
                    theme,
                );
            }
        }
    }
}

fn truncate_with_offset(s: &str, offset: usize, max_width: usize) -> String {
    if max_width == 0 || offset >= text_display_width(s) {
        return String::new();
    }

    let mut skipped_width = 0;
    let mut visible_width = 0;
    let mut truncated = String::new();

    for ch in s.chars() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);

        if skipped_width + char_width <= offset {
            skipped_width += char_width;
            continue;
        }

        if skipped_width < offset {
            skipped_width = offset;
            continue;
        }

        if visible_width + char_width > max_width {
            break;
        }

        truncated.push(ch);
        visible_width += char_width;
    }

    truncated
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
        fn ascii_input_truncates_substring(
            #[case] input: &str,
            #[case] offset: usize,
            #[case] max_width: usize,
            #[case] expected: &str,
        ) {
            let result = truncate_with_offset(input, offset, max_width);

            assert_eq!(result, expected);
        }

        #[rstest]
        #[case("日本語テスト", 0, 3, "日")]
        #[case("日本語テスト", 1, 3, "本")]
        #[case("日本語テスト", 2, 3, "本")]
        #[case("日本語テスト", 0, 1, "")]
        #[case("public.日本語_table", 0, 10, "public.日")]
        #[case("🎉table🎊", 0, 6, "🎉tabl")]
        fn unicode_input_truncates_visible_columns(
            #[case] input: &str,
            #[case] offset: usize,
            #[case] max_width: usize,
            #[case] expected: &str,
        ) {
            let result = truncate_with_offset(input, offset, max_width);

            assert_eq!(result, expected);
        }
    }
}
