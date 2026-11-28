use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::key_sequence::KeySequenceState;
use crate::model::shared::ui_state::scroll_max_offset;
use crate::policy::table_kind::max_explorer_table_label_width;
use crate::update::action::{CursorPosition, ScrollAmount, ScrollDirection, SelectMotion};

use super::explorer_item_count;

pub(in crate::update) fn select_explorer(
    state: &mut AppState,
    motion: SelectMotion,
) -> Vec<Effect> {
    match motion {
        SelectMotion::Next => {
            if state.ui.focused_pane() == FocusedPane::Explorer {
                let len = state.tables().len();
                if len > 0 && state.ui.explorer_selected() < len - 1 {
                    state
                        .ui
                        .set_explorer_selection(Some(state.ui.explorer_selected() + 1));
                }
            }
        }
        SelectMotion::Previous => {
            if state.ui.focused_pane() == FocusedPane::Explorer && !state.tables().is_empty() {
                let new_idx = state.ui.explorer_selected().saturating_sub(1);
                state.ui.set_explorer_selection(Some(new_idx));
            }
        }
        SelectMotion::First => {
            if state.ui.focused_pane() == FocusedPane::Explorer && !state.tables().is_empty() {
                state.ui.set_explorer_selection(Some(0));
            }
        }
        SelectMotion::Last => {
            if state.ui.focused_pane() == FocusedPane::Explorer {
                let len = state.tables().len();
                if len > 0 {
                    state.ui.set_explorer_selection(Some(len - 1));
                }
            }
        }
        SelectMotion::ViewportMiddle => {
            if state.ui.focused_pane() == FocusedPane::Explorer {
                let len = explorer_item_count(state);
                let visible = state.ui.explorer_visible_items();
                if len > 0 && visible > 0 {
                    let displayed =
                        visible.min(len.saturating_sub(state.ui.explorer_scroll_offset));
                    let target = state.ui.explorer_scroll_offset + displayed / 2;
                    state.ui.set_explorer_selection(Some(target));
                }
            }
        }
        SelectMotion::ViewportTop => {
            if state.ui.focused_pane() == FocusedPane::Explorer {
                let len = explorer_item_count(state);
                if len > 0 {
                    let target = state.ui.explorer_scroll_offset.min(len.saturating_sub(1));
                    state.ui.set_explorer_selection(Some(target));
                }
            }
        }
        SelectMotion::ViewportBottom => {
            if state.ui.focused_pane() == FocusedPane::Explorer {
                let len = explorer_item_count(state);
                let visible = state.ui.explorer_visible_items();
                if len > 0 && visible > 0 {
                    let displayed =
                        visible.min(len.saturating_sub(state.ui.explorer_scroll_offset));
                    let target = state.ui.explorer_scroll_offset + displayed.saturating_sub(1);
                    state.ui.set_explorer_selection(Some(target));
                }
            }
        }
        SelectMotion::HalfPageDown | SelectMotion::FullPageDown => {
            let len = explorer_item_count(state);
            let visible = state.ui.explorer_visible_items();
            let delta = match motion {
                SelectMotion::HalfPageDown => (visible / 2).max(1),
                _ => visible.max(1),
            };
            state.ui.scroll_explorer_page_down(len, delta);
        }
        SelectMotion::HalfPageUp | SelectMotion::FullPageUp => {
            let len = explorer_item_count(state);
            let visible = state.ui.explorer_visible_items();
            let delta = match motion {
                SelectMotion::HalfPageUp => (visible / 2).max(1),
                _ => visible.max(1),
            };
            state.ui.scroll_explorer_page_up(len, delta);
        }
    }
    vec![]
}

pub(in crate::update) fn scroll_to_cursor_explorer(
    state: &mut AppState,
    position: CursorPosition,
) -> Vec<Effect> {
    match position {
        CursorPosition::Center => {
            state.ui.key_sequence = KeySequenceState::Idle;
            let len = explorer_item_count(state);
            let visible = state.ui.explorer_visible_items();
            if len > 0 && visible > 0 {
                let selected = state.ui.explorer_selected();
                let max_offset = len.saturating_sub(visible);
                state.ui.explorer_scroll_offset =
                    selected.saturating_sub(visible / 2).min(max_offset);
            }
        }
        CursorPosition::Top => {
            state.ui.key_sequence = KeySequenceState::Idle;
            let len = explorer_item_count(state);
            let visible = state.ui.explorer_visible_items();
            if len > 0 && visible > 0 {
                let selected = state.ui.explorer_selected();
                let max_offset = len.saturating_sub(visible);
                state.ui.explorer_scroll_offset = selected.min(max_offset);
            }
        }
        CursorPosition::Bottom => {
            state.ui.key_sequence = KeySequenceState::Idle;
            let len = explorer_item_count(state);
            let visible = state.ui.explorer_visible_items();
            if len > 0 && visible > 0 {
                let selected = state.ui.explorer_selected();
                let max_offset = len.saturating_sub(visible);
                state.ui.explorer_scroll_offset = selected
                    .saturating_sub(visible.saturating_sub(1))
                    .min(max_offset);
            }
        }
    }
    vec![]
}

pub(in crate::update) fn scroll_explorer(
    state: &mut AppState,
    direction: ScrollDirection,
    amount: ScrollAmount,
) {
    match (direction, amount) {
        (ScrollDirection::Left, ScrollAmount::Line) => {
            state.ui.explorer_horizontal_offset =
                state.ui.explorer_horizontal_offset.saturating_sub(1);
        }
        (ScrollDirection::Right, ScrollAmount::Line) => {
            let max_name_width = max_explorer_table_label_width(
                state.tables(),
                state.session.active_database_type_or_default(),
            );
            let max_offset = scroll_max_offset(max_name_width, state.ui.explorer_content_width);
            if state.ui.explorer_horizontal_offset < max_offset {
                state.ui.explorer_horizontal_offset += 1;
            }
        }
        _ => debug_assert!(
            false,
            "unsupported Explorer scroll: {direction:?} {amount:?}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DatabaseMetadata, TableSummary};
    use crate::model::shared::key_sequence::Prefix;
    use crate::services::AppServices;
    use crate::test_support::endpoint;
    use crate::update::action::{Action, ScrollTarget, ScrollToCursorTarget};
    use crate::update::reducer::route;
    use rstest::rstest;
    use std::sync::Arc;
    use std::time::Instant;

    fn state_with_tables(count: usize, pane_height: u16) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.ui.set_explorer_pane_height(pane_height);
        state.focus_pane(FocusedPane::Explorer);
        let tables: Vec<TableSummary> = (0..count)
            .map(|i| TableSummary::new("public".to_string(), format!("table_{i}"), Some(0)))
            .collect();
        state.session.set_metadata(Some(Arc::new({
            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.table_summaries = tables;
            metadata
        })));
        state.ui.set_explorer_selection(Some(0));
        state
    }

    fn state_with_named_tables(names: &[&str], content_width: usize) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.focus_pane(FocusedPane::Explorer);
        state.ui.explorer_content_width = content_width;
        let tables: Vec<TableSummary> = names
            .iter()
            .map(|name| TableSummary::new("public".to_string(), (*name).to_string(), Some(0)))
            .collect();
        state.session.set_metadata(Some(Arc::new({
            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.table_summaries = tables;
            metadata
        })));
        state
    }

    mod page_scroll {
        use super::*;

        #[test]
        fn half_page_down_jumps_by_correct_delta() {
            let mut state = state_with_tables(50, 23);
            route(
                &mut state,
                Action::Select(SelectMotion::HalfPageDown),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 10);
        }

        #[test]
        fn half_page_down_clamped_at_last() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selection(Some(45));

            route(
                &mut state,
                Action::Select(SelectMotion::HalfPageDown),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 49);
        }

        #[test]
        fn half_page_up_clamped_at_zero() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selection(Some(3));

            route(
                &mut state,
                Action::Select(SelectMotion::HalfPageUp),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 0);
        }

        #[test]
        fn full_page_down_jumps_by_visible() {
            let mut state = state_with_tables(50, 23);
            route(
                &mut state,
                Action::Select(SelectMotion::FullPageDown),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 20);
        }

        #[test]
        fn empty_list_does_nothing() {
            let mut state = AppState::new("test".to_string());
            state.ui.set_explorer_pane_height(23);

            route(
                &mut state,
                Action::Select(SelectMotion::HalfPageDown),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 0);
        }

        #[test]
        fn zero_height_pane_is_noop() {
            let mut state = state_with_tables(50, 0);
            route(
                &mut state,
                Action::Select(SelectMotion::HalfPageDown),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 0);
            assert_eq!(state.ui.explorer_scroll_offset, 0);
        }

        #[test]
        fn half_page_down_moves_both_selection_and_scroll() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selected_raw(15);
            state.ui.explorer_scroll_offset = 10;

            route(
                &mut state,
                Action::Select(SelectMotion::HalfPageDown),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 25);
            assert_eq!(state.ui.explorer_scroll_offset, 20);
        }

        #[test]
        fn half_page_up_moves_both_selection_and_scroll() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selected_raw(25);
            state.ui.explorer_scroll_offset = 20;

            route(
                &mut state,
                Action::Select(SelectMotion::HalfPageUp),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 15);
            assert_eq!(state.ui.explorer_scroll_offset, 10);
        }

        #[test]
        fn half_page_down_preserves_relative_position() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selected_raw(15);
            state.ui.explorer_scroll_offset = 10;

            route(
                &mut state,
                Action::Select(SelectMotion::HalfPageDown),
                Instant::now(),
                &AppServices::stub(),
            );

            let relative = state.ui.explorer_selected() - state.ui.explorer_scroll_offset;
            assert_eq!(relative, 5);
        }

        #[test]
        fn data_fewer_than_viewport_scroll_stays_zero() {
            let mut state = state_with_tables(10, 23);
            state.ui.set_explorer_selected_raw(3);

            route(
                &mut state,
                Action::Select(SelectMotion::HalfPageDown),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 9);
            assert_eq!(state.ui.explorer_scroll_offset, 0);
        }

        #[test]
        fn full_page_down_moves_both_selection_and_scroll() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selected_raw(10);
            state.ui.explorer_scroll_offset = 5;

            route(
                &mut state,
                Action::Select(SelectMotion::FullPageDown),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 30);
            assert_eq!(state.ui.explorer_scroll_offset, 25);
        }

        #[test]
        fn full_page_up_moves_both_selection_and_scroll() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selected_raw(30);
            state.ui.explorer_scroll_offset = 25;

            route(
                &mut state,
                Action::Select(SelectMotion::FullPageUp),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 10);
            assert_eq!(state.ui.explorer_scroll_offset, 5);
        }

        #[test]
        fn full_page_down_clamps_near_bottom() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selected_raw(40);
            state.ui.explorer_scroll_offset = 25;

            route(
                &mut state,
                Action::Select(SelectMotion::FullPageDown),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 49);
            assert_eq!(state.ui.explorer_scroll_offset, 30);
        }

        #[test]
        fn full_page_up_clamps_near_top() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selected_raw(10);
            state.ui.explorer_scroll_offset = 5;

            route(
                &mut state,
                Action::Select(SelectMotion::FullPageUp),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 0);
            assert_eq!(state.ui.explorer_scroll_offset, 0);
        }
    }

    mod viewport_selection {
        use super::*;

        #[test]
        fn select_middle_moves_to_viewport_center() {
            let mut state = state_with_tables(50, 23);
            route(
                &mut state,
                Action::Select(SelectMotion::ViewportMiddle),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 10);
            assert_eq!(state.ui.explorer_scroll_offset, 0);
        }

        #[test]
        fn select_middle_respects_scroll_offset() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_scroll_offset = 15;
            state.ui.set_explorer_selected_raw(15);
            route(
                &mut state,
                Action::Select(SelectMotion::ViewportMiddle),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 25);
            assert_eq!(state.ui.explorer_scroll_offset, 15);
        }

        #[test]
        fn select_viewport_top_moves_to_first_visible_item() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_scroll_offset = 10;
            state.ui.set_explorer_selected_raw(20);

            route(
                &mut state,
                Action::Select(SelectMotion::ViewportTop),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 10);
        }

        #[test]
        fn select_viewport_bottom_moves_to_last_visible_item() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_scroll_offset = 10;
            state.ui.set_explorer_selected_raw(15);

            route(
                &mut state,
                Action::Select(SelectMotion::ViewportBottom),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 29);
        }

        #[test]
        fn select_viewport_bottom_clamps_to_last_displayed_item() {
            let mut state = state_with_tables(10, 23);
            route(
                &mut state,
                Action::Select(SelectMotion::ViewportBottom),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 9);
        }

        #[test]
        fn select_viewport_middle_uses_displayed_count_near_end() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_scroll_offset = 40;
            state.ui.set_explorer_selected_raw(40);

            route(
                &mut state,
                Action::Select(SelectMotion::ViewportMiddle),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 45);
        }

        #[test]
        fn select_viewport_bottom_uses_displayed_count_near_end() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_scroll_offset = 40;
            state.ui.set_explorer_selected_raw(40);

            route(
                &mut state,
                Action::Select(SelectMotion::ViewportBottom),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_selected(), 49);
        }
    }

    mod cursor_reposition {
        use super::*;

        #[test]
        fn scroll_cursor_center_centers_viewport_on_selected() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selected_raw(30);
            state.ui.explorer_scroll_offset = 30;
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            route(
                &mut state,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Center,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_scroll_offset, 20);
            assert_eq!(state.ui.key_sequence, KeySequenceState::Idle);
        }

        #[test]
        fn scroll_cursor_top_puts_selected_at_top() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selected_raw(15);
            state.ui.explorer_scroll_offset = 0;
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            route(
                &mut state,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Top,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_scroll_offset, 15);
            assert_eq!(state.ui.key_sequence, KeySequenceState::Idle);
        }

        #[test]
        fn scroll_cursor_bottom_puts_selected_at_bottom() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selected_raw(25);
            state.ui.explorer_scroll_offset = 25;
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            route(
                &mut state,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Bottom,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_scroll_offset, 6);
            assert_eq!(state.ui.key_sequence, KeySequenceState::Idle);
        }

        #[test]
        fn scroll_cursor_top_clamps_to_max_offset() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selected_raw(45);
            state.ui.explorer_scroll_offset = 30;
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            route(
                &mut state,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Top,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.explorer_scroll_offset, 30);
            assert_eq!(state.ui.key_sequence, KeySequenceState::Idle);
        }
    }

    mod horizontal_scroll {
        use super::*;
        use crate::domain::{ConnectionId, TableKindInfo};
        use crate::policy::table_kind::{explorer_table_label, explorer_table_label_width};

        #[rstest]
        #[case(&["abcdefghij"], 4, 32)]
        #[case(&["日本語テスト"], 3, 32)]
        fn repeated_right_scroll_clamps_at_last_visible_column(
            #[case] names: &[&str],
            #[case] content_width: usize,
            #[case] presses: usize,
        ) {
            let mut state = state_with_named_tables(names, content_width);
            let expected = explorer_table_label_width(
                &state.tables()[0],
                state.session.active_database_type_or_default(),
            )
            .saturating_sub(content_width);

            for _ in 0..presses {
                route(
                    &mut state,
                    Action::Scroll {
                        target: ScrollTarget::Explorer,
                        direction: ScrollDirection::Right,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                    &AppServices::stub(),
                );
            }

            assert_eq!(state.ui.explorer_horizontal_offset, expected);
        }

        #[test]
        fn right_presses_past_end_do_not_increase_offset() {
            let mut state = state_with_named_tables(&["abcdefghij"], 4);
            state.ui.explorer_horizontal_offset = explorer_table_label_width(
                &state.tables()[0],
                state.session.active_database_type_or_default(),
            )
            .saturating_sub(state.ui.explorer_content_width);

            for _ in 0..3 {
                route(
                    &mut state,
                    Action::Scroll {
                        target: ScrollTarget::Explorer,
                        direction: ScrollDirection::Right,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                    &AppServices::stub(),
                );
            }

            assert_eq!(
                state.ui.explorer_horizontal_offset,
                explorer_table_label_width(
                    &state.tables()[0],
                    state.session.active_database_type_or_default(),
                )
                .saturating_sub(state.ui.explorer_content_width)
            );
        }

        #[test]
        fn left_press_after_end_recovers_one_column() {
            let mut state = state_with_named_tables(&["abcdefghij"], 4);
            state.ui.explorer_horizontal_offset = explorer_table_label_width(
                &state.tables()[0],
                state.session.active_database_type_or_default(),
            )
            .saturating_sub(state.ui.explorer_content_width);

            route(
                &mut state,
                Action::Scroll {
                    target: ScrollTarget::Explorer,
                    direction: ScrollDirection::Left,
                    amount: ScrollAmount::Line,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(
                state.ui.explorer_horizontal_offset,
                explorer_table_label_width(
                    &state.tables()[0],
                    state.session.active_database_type_or_default(),
                )
                .saturating_sub(state.ui.explorer_content_width)
                .saturating_sub(1)
            );
        }

        #[test]
        fn right_scroll_uses_sqlite_table_name_width() {
            let mut state = AppState::new("test".to_string());
            state.focus_pane(FocusedPane::Explorer);
            state.ui.explorer_content_width = 4;
            state.session.activate_connection(
                &ConnectionId::from_string("sqlite-test"),
                "sqlite",
                &endpoint::sqlite("sqlite:///tmp/test.db"),
            );
            let summary = TableSummary::new("main".to_string(), "settings".to_string(), None)
                .with_kind_info(TableKindInfo {
                    without_rowid: true,
                    ..TableKindInfo::default()
                });
            state.session.set_metadata(Some(Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = vec![summary];
                metadata
            })));

            let expected = explorer_table_label_width(
                &state.tables()[0],
                state.session.active_database_type_or_default(),
            )
            .saturating_sub(4);
            for _ in 0..32 {
                route(
                    &mut state,
                    Action::Scroll {
                        target: ScrollTarget::Explorer,
                        direction: ScrollDirection::Right,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                    &AppServices::stub(),
                );
            }

            assert_eq!(state.ui.explorer_horizontal_offset, expected);
            assert_eq!(expected, 4);
            assert_eq!(
                explorer_table_label(
                    &state.tables()[0],
                    state.session.active_database_type_or_default(),
                ),
                "settings"
            );
        }
    }
}
