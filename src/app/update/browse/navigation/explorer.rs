use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::key_sequence::KeySequenceState;
use crate::app::update::action::{
    Action, CursorPosition, ScrollAmount, ScrollDirection, ScrollTarget, ScrollToCursorTarget,
    SelectMotion,
};

use super::explorer_item_count;

pub fn reduce(state: &mut AppState, action: &Action) -> Option<Vec<Effect>> {
    match action {
        Action::Select(SelectMotion::Next) => {
            if state.ui.focused_pane == FocusedPane::Explorer {
                let len = state.tables().len();
                if len > 0 && state.ui.explorer_selected < len - 1 {
                    state
                        .ui
                        .set_explorer_selection(Some(state.ui.explorer_selected + 1));
                }
            }
            Some(vec![])
        }
        Action::Select(SelectMotion::Previous) => {
            if state.ui.focused_pane == FocusedPane::Explorer && !state.tables().is_empty() {
                let new_idx = state.ui.explorer_selected.saturating_sub(1);
                state.ui.set_explorer_selection(Some(new_idx));
            }
            Some(vec![])
        }
        Action::Select(SelectMotion::First) => {
            if state.ui.focused_pane == FocusedPane::Explorer && !state.tables().is_empty() {
                state.ui.set_explorer_selection(Some(0));
            }
            Some(vec![])
        }
        Action::Select(SelectMotion::Last) => {
            if state.ui.focused_pane == FocusedPane::Explorer {
                let len = state.tables().len();
                if len > 0 {
                    state.ui.set_explorer_selection(Some(len - 1));
                }
            }
            Some(vec![])
        }
        Action::Select(SelectMotion::ViewportMiddle) => {
            if state.ui.focused_pane == FocusedPane::Explorer {
                let len = explorer_item_count(state);
                let visible = state.ui.explorer_visible_items();
                if len > 0 && visible > 0 {
                    let displayed =
                        visible.min(len.saturating_sub(state.ui.explorer_scroll_offset));
                    let target = state.ui.explorer_scroll_offset + displayed / 2;
                    state.ui.set_explorer_selection(Some(target));
                }
            }
            Some(vec![])
        }
        Action::Select(SelectMotion::ViewportTop) => {
            if state.ui.focused_pane == FocusedPane::Explorer {
                let len = explorer_item_count(state);
                if len > 0 {
                    let target = state.ui.explorer_scroll_offset.min(len.saturating_sub(1));
                    state.ui.set_explorer_selection(Some(target));
                }
            }
            Some(vec![])
        }
        Action::Select(SelectMotion::ViewportBottom) => {
            if state.ui.focused_pane == FocusedPane::Explorer {
                let len = explorer_item_count(state);
                let visible = state.ui.explorer_visible_items();
                if len > 0 && visible > 0 {
                    let displayed =
                        visible.min(len.saturating_sub(state.ui.explorer_scroll_offset));
                    let target = state.ui.explorer_scroll_offset + displayed.saturating_sub(1);
                    state.ui.set_explorer_selection(Some(target));
                }
            }
            Some(vec![])
        }

        Action::ScrollToCursor {
            target: ScrollToCursorTarget::Explorer,
            position: CursorPosition::Center,
        } => {
            state.ui.key_sequence = KeySequenceState::Idle;
            let len = explorer_item_count(state);
            let visible = state.ui.explorer_visible_items();
            if len > 0 && visible > 0 {
                let selected = state.ui.explorer_selected;
                let max_offset = len.saturating_sub(visible);
                state.ui.explorer_scroll_offset =
                    selected.saturating_sub(visible / 2).min(max_offset);
            }
            Some(vec![])
        }
        Action::ScrollToCursor {
            target: ScrollToCursorTarget::Explorer,
            position: CursorPosition::Top,
        } => {
            state.ui.key_sequence = KeySequenceState::Idle;
            let len = explorer_item_count(state);
            let visible = state.ui.explorer_visible_items();
            if len > 0 && visible > 0 {
                let selected = state.ui.explorer_selected;
                let max_offset = len.saturating_sub(visible);
                state.ui.explorer_scroll_offset = selected.min(max_offset);
            }
            Some(vec![])
        }
        Action::ScrollToCursor {
            target: ScrollToCursorTarget::Explorer,
            position: CursorPosition::Bottom,
        } => {
            state.ui.key_sequence = KeySequenceState::Idle;
            let len = explorer_item_count(state);
            let visible = state.ui.explorer_visible_items();
            if len > 0 && visible > 0 {
                let selected = state.ui.explorer_selected;
                let max_offset = len.saturating_sub(visible);
                state.ui.explorer_scroll_offset = selected
                    .saturating_sub(visible.saturating_sub(1))
                    .min(max_offset);
            }
            Some(vec![])
        }

        Action::Select(SelectMotion::HalfPageDown) => {
            let len = explorer_item_count(state);
            if len == 0 {
                return Some(vec![]);
            }
            let visible = state.ui.explorer_visible_items();
            if visible == 0 {
                return Some(vec![]);
            }
            let delta = (visible / 2).max(1);
            let max_idx = len.saturating_sub(1);
            let max_offset = len.saturating_sub(visible);
            let new_idx = (state.ui.explorer_selected + delta).min(max_idx);
            state.ui.explorer_selected = new_idx;
            state.ui.explorer_scroll_offset =
                (state.ui.explorer_scroll_offset + delta).min(max_offset);
            Some(vec![])
        }
        Action::Select(SelectMotion::HalfPageUp) => {
            let len = explorer_item_count(state);
            if len == 0 {
                return Some(vec![]);
            }
            let visible = state.ui.explorer_visible_items();
            if visible == 0 {
                return Some(vec![]);
            }
            let delta = (visible / 2).max(1);
            let new_idx = state.ui.explorer_selected.saturating_sub(delta);
            state.ui.explorer_selected = new_idx;
            state.ui.explorer_scroll_offset = state.ui.explorer_scroll_offset.saturating_sub(delta);
            Some(vec![])
        }
        Action::Select(SelectMotion::FullPageDown) => {
            let len = explorer_item_count(state);
            if len == 0 {
                return Some(vec![]);
            }
            let visible = state.ui.explorer_visible_items();
            if visible == 0 {
                return Some(vec![]);
            }
            let delta = visible.max(1);
            let max_idx = len.saturating_sub(1);
            let max_offset = len.saturating_sub(visible);
            let new_idx = (state.ui.explorer_selected + delta).min(max_idx);
            state.ui.explorer_selected = new_idx;
            state.ui.explorer_scroll_offset =
                (state.ui.explorer_scroll_offset + delta).min(max_offset);
            Some(vec![])
        }
        Action::Select(SelectMotion::FullPageUp) => {
            let len = explorer_item_count(state);
            if len == 0 {
                return Some(vec![]);
            }
            let visible = state.ui.explorer_visible_items();
            if visible == 0 {
                return Some(vec![]);
            }
            let delta = visible.max(1);
            let new_idx = state.ui.explorer_selected.saturating_sub(delta);
            state.ui.explorer_selected = new_idx;
            state.ui.explorer_scroll_offset = state.ui.explorer_scroll_offset.saturating_sub(delta);
            Some(vec![])
        }

        Action::Scroll {
            target: ScrollTarget::Explorer,
            direction: ScrollDirection::Left,
            amount: ScrollAmount::Line,
        } => {
            state.ui.explorer_horizontal_offset =
                state.ui.explorer_horizontal_offset.saturating_sub(1);
            Some(vec![])
        }
        Action::Scroll {
            target: ScrollTarget::Explorer,
            direction: ScrollDirection::Right,
            amount: ScrollAmount::Line,
        } => {
            let max_name_width = state
                .tables()
                .iter()
                .map(|t| t.qualified_name().len())
                .max()
                .unwrap_or(0);
            if state.ui.explorer_horizontal_offset < max_name_width {
                state.ui.explorer_horizontal_offset += 1;
            }
            Some(vec![])
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::key_sequence::Prefix;
    use crate::app::services::AppServices;
    use crate::app::update::browse::navigation::reduce_navigation;
    use crate::domain::{DatabaseMetadata, TableSummary};
    use std::sync::Arc;
    use std::time::Instant;

    mod explorer_page_scroll {
        use super::*;

        fn state_with_tables(count: usize, pane_height: u16) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.explorer_pane_height = pane_height;
            state.ui.focused_pane = FocusedPane::Explorer;
            let tables: Vec<TableSummary> = (0..count)
                .map(|i| {
                    TableSummary::new("public".to_string(), format!("table_{i}"), Some(0), false)
                })
                .collect();
            state.session.set_metadata(Some(Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                table_summaries: tables,
                fetched_at: Instant::now(),
            })));
            state.ui.set_explorer_selection(Some(0));
            state
        }

        #[test]
        fn half_page_down_jumps_by_correct_delta() {
            let mut state = state_with_tables(50, 23);
            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::HalfPageDown),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 10);
        }

        #[test]
        fn half_page_down_clamped_at_last() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selection(Some(45));

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::HalfPageDown),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 49);
        }

        #[test]
        fn half_page_up_clamped_at_zero() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selection(Some(3));

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::HalfPageUp),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn full_page_down_jumps_by_visible() {
            let mut state = state_with_tables(50, 23);
            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::FullPageDown),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 20);
        }

        #[test]
        fn empty_list_does_nothing() {
            let mut state = AppState::new("test".to_string());
            state.ui.explorer_pane_height = 23;

            let effects = reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::HalfPageDown),
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn zero_height_pane_is_noop() {
            let mut state = state_with_tables(50, 0);
            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::HalfPageDown),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 0);
            assert_eq!(state.ui.explorer_scroll_offset, 0);
        }

        #[test]
        fn half_page_down_moves_both_selection_and_scroll() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_selected = 15;
            state.ui.explorer_scroll_offset = 10;

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::HalfPageDown),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 25);
            assert_eq!(state.ui.explorer_scroll_offset, 20);
        }

        #[test]
        fn half_page_up_moves_both_selection_and_scroll() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_selected = 25;
            state.ui.explorer_scroll_offset = 20;

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::HalfPageUp),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 15);
            assert_eq!(state.ui.explorer_scroll_offset, 10);
        }

        #[test]
        fn half_page_down_preserves_relative_position() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_selected = 15;
            state.ui.explorer_scroll_offset = 10;

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::HalfPageDown),
                &AppServices::stub(),
                Instant::now(),
            );

            let relative = state.ui.explorer_selected - state.ui.explorer_scroll_offset;
            assert_eq!(relative, 5);
        }

        #[test]
        fn data_fewer_than_viewport_scroll_stays_zero() {
            let mut state = state_with_tables(10, 23);
            state.ui.explorer_selected = 3;

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::HalfPageDown),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 9);
            assert_eq!(state.ui.explorer_scroll_offset, 0);
        }

        #[test]
        fn full_page_down_moves_both_selection_and_scroll() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_selected = 10;
            state.ui.explorer_scroll_offset = 5;

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::FullPageDown),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 30);
            assert_eq!(state.ui.explorer_scroll_offset, 25);
        }

        #[test]
        fn full_page_up_moves_both_selection_and_scroll() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_selected = 30;
            state.ui.explorer_scroll_offset = 25;

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::FullPageUp),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 10);
            assert_eq!(state.ui.explorer_scroll_offset, 5);
        }

        #[test]
        fn full_page_down_clamps_near_bottom() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_selected = 40;
            state.ui.explorer_scroll_offset = 25;

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::FullPageDown),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 49);
            assert_eq!(state.ui.explorer_scroll_offset, 30);
        }

        #[test]
        fn full_page_up_clamps_near_top() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_selected = 10;
            state.ui.explorer_scroll_offset = 5;

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::FullPageUp),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 0);
            assert_eq!(state.ui.explorer_scroll_offset, 0);
        }

        #[test]
        fn select_middle_moves_to_viewport_center() {
            let mut state = state_with_tables(50, 23);
            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::ViewportMiddle),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 10);
            assert_eq!(state.ui.explorer_scroll_offset, 0);
        }

        #[test]
        fn select_middle_respects_scroll_offset() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_scroll_offset = 15;
            state.ui.explorer_selected = 15;
            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::ViewportMiddle),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 25);
            assert_eq!(state.ui.explorer_scroll_offset, 15);
        }

        #[test]
        fn select_viewport_top_moves_to_first_visible_item() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_scroll_offset = 10;
            state.ui.explorer_selected = 20;

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::ViewportTop),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 10);
        }

        #[test]
        fn select_viewport_bottom_moves_to_last_visible_item() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_scroll_offset = 10;
            state.ui.explorer_selected = 15;

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::ViewportBottom),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 29);
        }

        #[test]
        fn select_viewport_bottom_clamps_to_last_displayed_item() {
            let mut state = state_with_tables(10, 23);
            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::ViewportBottom),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 9);
        }

        #[test]
        fn select_viewport_middle_uses_displayed_count_near_end() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_scroll_offset = 40;
            state.ui.explorer_selected = 40;

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::ViewportMiddle),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 45);
        }

        #[test]
        fn select_viewport_bottom_uses_displayed_count_near_end() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_scroll_offset = 40;
            state.ui.explorer_selected = 40;

            reduce_navigation(
                &mut state,
                &Action::Select(SelectMotion::ViewportBottom),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 49);
        }

        #[test]
        fn scroll_cursor_center_centers_viewport_on_selected() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_selected = 30;
            state.ui.explorer_scroll_offset = 30;
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            reduce_navigation(
                &mut state,
                &Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Center,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_scroll_offset, 20);
            assert_eq!(state.ui.key_sequence, KeySequenceState::Idle);
        }

        #[test]
        fn scroll_cursor_top_puts_selected_at_top() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_selected = 15;
            state.ui.explorer_scroll_offset = 0;
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            reduce_navigation(
                &mut state,
                &Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Top,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_scroll_offset, 15);
            assert_eq!(state.ui.key_sequence, KeySequenceState::Idle);
        }

        #[test]
        fn scroll_cursor_bottom_puts_selected_at_bottom() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_selected = 25;
            state.ui.explorer_scroll_offset = 25;
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            reduce_navigation(
                &mut state,
                &Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Bottom,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_scroll_offset, 6);
            assert_eq!(state.ui.key_sequence, KeySequenceState::Idle);
        }

        #[test]
        fn scroll_cursor_top_clamps_to_max_offset() {
            let mut state = state_with_tables(50, 23);
            state.ui.explorer_selected = 45;
            state.ui.explorer_scroll_offset = 30;
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            reduce_navigation(
                &mut state,
                &Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Top,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_scroll_offset, 30);
            assert_eq!(state.ui.key_sequence, KeySequenceState::Idle);
        }
    }
}
