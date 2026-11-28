use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{List, ListItem, ListState};
use unicode_segmentation::UnicodeSegmentation;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::motion::ScrolledPane;
use crate::app::model::shared::ui_state::{
    explorer_content_width_from_inner_width, scroll_max_offset, text_display_width,
};
use crate::app::policy::table_kind::{explorer_table_label, max_explorer_table_label_width};
use crate::app::update::input::keymap::{BrowseShortcut, KeyContext, browse_shortcut};
use crate::domain::MetadataState;
use crate::primitives::atoms::Icon;
use crate::theme::ThemePalette;

use crate::primitives::atoms::panel_block;

pub struct Explorer;

impl Explorer {
    pub fn render(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        now: Instant,
        theme: &ThemePalette,
    ) -> usize {
        let is_focused = state.ui.focused_pane() == FocusedPane::Explorer;
        let block = panel_block(" [1] Explorer ", is_focused, theme);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let is_error = matches!(state.session.metadata_state(), MetadataState::Error);
        let has_cached_data =
            !is_error && state.session.metadata().is_some() && !state.tables().is_empty();
        Self::render_tables_section(frame, inner, state, has_cached_data, now, theme)
    }

    fn render_tables_section(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        has_cached_data: bool,
        now: Instant,
        theme: &ThemePalette,
    ) -> usize {
        let mode = state.settings.saved_icon_mode();
        let available_width = explorer_content_width_from_inner_width(area.width);
        let icon_width = text_display_width(Icon::Table.prefix(mode));
        let show_icons = available_width >= icon_width + 2;
        let content_width = if show_icons {
            available_width - icon_width
        } else {
            available_width
        };

        let max_name_width = max_explorer_table_label_width(
            state.tables(),
            state.session.active_database_type_or_default(),
        );
        let max_offset = scroll_max_offset(max_name_width, content_width);
        let h_offset = state.ui.explorer_horizontal_offset().min(max_offset);

        let items: Vec<ListItem> = if has_cached_data {
            state
                .tables()
                .iter()
                .map(|summary| {
                    let name = explorer_table_label(
                        summary,
                        state.session.active_database_type_or_default(),
                    );
                    let displayed = truncate_with_offset(&name, h_offset, content_width);
                    let icon = if show_icons {
                        Icon::from(summary.kind_info.kind).prefix(mode)
                    } else {
                        ""
                    };
                    ListItem::new(format!("{icon}{displayed}"))
                })
                .collect()
        } else {
            match &state.session.metadata_state() {
                MetadataState::Loading => {
                    vec![ListItem::new(" Loading metadata...")]
                }
                MetadataState::Error => {
                    let context = KeyContext::for_browse_state(state);
                    let action_hint = if state.session.endpoint().is_some() {
                        let mut hints = Vec::new();
                        if let Some(key) = browse_shortcut(BrowseShortcut::ReloadMetadata, &context)
                        {
                            hints.push(format!("{key}: retry"));
                        }
                        if let Some(key) = browse_shortcut(BrowseShortcut::ErrorDetails, &context) {
                            hints.push(format!("{key}: details"));
                        }
                        if hints.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", hints.join(", "))
                        }
                    } else {
                        browse_shortcut(BrowseShortcut::SelectConnection, &context)
                            .map_or_else(String::new, |key| format!(" ({key}: select connection)"))
                    };
                    vec![
                        ListItem::new(" Metadata load failed"),
                        ListItem::new(action_hint),
                    ]
                }
                MetadataState::Loaded => {
                    vec![ListItem::new(" No tables found")]
                }
                MetadataState::NotLoaded => {
                    let context = KeyContext::for_browse_state(state);
                    let (shortcut, action) = if state.session.endpoint().is_some() {
                        (BrowseShortcut::ReloadMetadata, "load metadata")
                    } else {
                        (BrowseShortcut::SelectConnection, "select a connection")
                    };
                    let action_hint = browse_shortcut(shortcut, &context)
                        .map_or_else(String::new, |key| format!(" Press '{key}' to {action}"));
                    vec![ListItem::new(action_hint)]
                }
            }
        };

        let list = List::new(items)
            .style(Style::default().fg(theme.semantic.text.primary))
            .highlight_style(
                Style::default()
                    .fg(theme.semantic.text.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let scroll_offset = state.motion.scroll_offset(
            ScrolledPane::Explorer,
            state.ui.explorer_scroll_offset(),
            now,
        );
        let animating = scroll_offset != state.ui.explorer_scroll_offset();
        let visible_rows = area.height as usize;
        // The list widget pulls its window back to the selection, so the selection stays hidden until the animated window reaches it.
        let selected = has_cached_data
            .then(|| state.ui.explorer_selected())
            .filter(|&selected| {
                !animating || (scroll_offset..scroll_offset + visible_rows).contains(&selected)
            });
        let mut list_state = ListState::default()
            .with_selected(selected)
            .with_offset(scroll_offset);
        frame.render_stateful_widget(list, area, &mut list_state);

        // Render scrollbars
        if has_cached_data {
            let total_items = state.tables().len();
            let viewport_size = area.height.saturating_sub(1) as usize; // Reserve for horizontal scrollbar

            if total_items > viewport_size {
                use crate::primitives::atoms::scroll_indicator::{
                    VerticalScrollParams, render_vertical_scroll_indicator_bar,
                };
                let has_h_scroll = max_name_width > content_width;
                render_vertical_scroll_indicator_bar(
                    frame,
                    area,
                    VerticalScrollParams {
                        position: scroll_offset,
                        viewport_size,
                        total_items,
                        has_horizontal_scrollbar: has_h_scroll,
                    },
                    theme,
                );
            }

            if max_name_width > content_width {
                use crate::primitives::atoms::scroll_indicator::{
                    HorizontalScrollParams, render_horizontal_scroll_indicator,
                };
                render_horizontal_scroll_indicator(
                    frame,
                    area,
                    HorizontalScrollParams {
                        position: h_offset,
                        viewport_size: content_width,
                        total_items: max_name_width,
                        label: "x",
                    },
                    theme,
                );
            }
        }
        content_width
    }
}

fn truncate_with_offset(s: &str, offset: usize, max_width: usize) -> String {
    let mut position = 0;
    let mut visible_width = 0;
    let mut truncated = String::new();

    for grapheme in s.graphemes(true) {
        let width = text_display_width(grapheme);
        if position < offset {
            position += width;
            continue;
        }
        if visible_width + width > max_width {
            break;
        }
        truncated.push_str(grapheme);
        visible_width += width;
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
        #[case("👩‍💻table", 0, 3, "👩‍💻t")]
        #[case("👩‍💻table", 1, 3, "tab")]
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
