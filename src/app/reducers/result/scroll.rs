use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::state::AppState;
use crate::app::viewport::{calculate_next_column_offset, calculate_prev_column_offset};

pub(super) fn result_row_count(state: &AppState) -> usize {
    state
        .query
        .visible_result()
        .map(|r| r.rows.len())
        .unwrap_or(0)
}

pub(super) fn result_col_count(state: &AppState) -> usize {
    state
        .query
        .visible_result()
        .map(|r| r.columns.len())
        .unwrap_or(0)
}

pub(super) fn result_max_scroll(state: &AppState) -> usize {
    let visible = state.result_visible_rows();
    result_row_count(state).saturating_sub(visible)
}

fn ensure_row_visible(state: &mut AppState) {
    if let Some(row) = state.result_interaction.selection().row() {
        let visible = state.result_visible_rows();
        if visible == 0 {
            return;
        }
        if row < state.result_interaction.scroll_offset {
            state.result_interaction.scroll_offset = row;
        } else if row >= state.result_interaction.scroll_offset + visible {
            state.result_interaction.scroll_offset = row - visible + 1;
        }
    }
}

fn move_row_or_scroll(state: &mut AppState, new_row: usize, scroll_fn: impl FnOnce(&mut AppState)) {
    if state.result_interaction.selection().row().is_some() {
        state.result_interaction.move_row(new_row);
        ensure_row_visible(state);
    } else {
        scroll_fn(state);
    }
}

pub fn reduce(state: &mut AppState, action: &Action) -> Option<Vec<Effect>> {
    match action {
        Action::ResultScrollUp => {
            let new_row = state
                .result_interaction
                .selection()
                .row()
                .and_then(|r| r.checked_sub(1));
            match new_row {
                Some(r) => move_row_or_scroll(state, r, |_| {}),
                None if state.result_interaction.selection().row().is_none() => {
                    state.result_interaction.scroll_offset =
                        state.result_interaction.scroll_offset.saturating_sub(1);
                }
                _ => {} // row == 0, no-op
            }
            Some(vec![])
        }
        Action::ResultScrollDown => {
            let max_row = result_row_count(state).saturating_sub(1);
            let new_row = state
                .result_interaction
                .selection()
                .row()
                .map(|r| (r + 1).min(max_row))
                .unwrap_or(0);
            move_row_or_scroll(state, new_row, |s| {
                let max_scroll = result_max_scroll(s);
                if s.result_interaction.scroll_offset < max_scroll {
                    s.result_interaction.scroll_offset += 1;
                }
            });
            Some(vec![])
        }
        Action::ResultScrollTop => {
            move_row_or_scroll(state, 0, |s| s.result_interaction.scroll_offset = 0);
            Some(vec![])
        }
        Action::ResultScrollBottom => {
            let max_row = result_row_count(state).saturating_sub(1);
            let max_scroll = result_max_scroll(state);
            move_row_or_scroll(state, max_row, |s| {
                s.result_interaction.scroll_offset = max_scroll
            });
            Some(vec![])
        }
        Action::ResultScrollViewportMiddle => {
            if state.result_interaction.selection().row().is_some() {
                let visible = state.result_visible_rows();
                let total = result_row_count(state);
                let offset = state.result_interaction.scroll_offset;
                let displayed = visible.min(total.saturating_sub(offset));
                let target_row = offset + displayed / 2;
                state.result_interaction.move_row(target_row);
                ensure_row_visible(state);
            }
            Some(vec![])
        }
        Action::ResultScrollViewportTop => {
            if state.result_interaction.selection().row().is_some() {
                let target = state.result_interaction.scroll_offset;
                state.result_interaction.move_row(target);
                ensure_row_visible(state);
            }
            Some(vec![])
        }
        Action::ResultScrollViewportBottom => {
            if state.result_interaction.selection().row().is_some() {
                let visible = state.result_visible_rows();
                let total = result_row_count(state);
                let offset = state.result_interaction.scroll_offset;
                let displayed = visible.min(total.saturating_sub(offset));
                let target = offset + displayed.saturating_sub(1);
                state.result_interaction.move_row(target);
                ensure_row_visible(state);
            }
            Some(vec![])
        }
        Action::ResultScrollHalfPageDown => {
            let visible = state.result_visible_rows();
            if visible == 0 {
                return Some(vec![]);
            }
            let delta = (visible / 2).max(1);
            if let Some(row) = state.result_interaction.selection().row() {
                let max_row = result_row_count(state).saturating_sub(1);
                let max_scroll = result_max_scroll(state);
                state
                    .result_interaction
                    .move_row((row + delta).min(max_row));
                state.result_interaction.scroll_offset =
                    (state.result_interaction.scroll_offset + delta).min(max_scroll);
            } else {
                let max = result_max_scroll(state);
                state.result_interaction.scroll_offset =
                    (state.result_interaction.scroll_offset + delta).min(max);
            }
            Some(vec![])
        }
        Action::ResultScrollHalfPageUp => {
            let visible = state.result_visible_rows();
            if visible == 0 {
                return Some(vec![]);
            }
            let delta = (visible / 2).max(1);
            if let Some(row) = state.result_interaction.selection().row() {
                state.result_interaction.move_row(row.saturating_sub(delta));
                state.result_interaction.scroll_offset =
                    state.result_interaction.scroll_offset.saturating_sub(delta);
            } else {
                state.result_interaction.scroll_offset =
                    state.result_interaction.scroll_offset.saturating_sub(delta);
            }
            Some(vec![])
        }
        Action::ResultScrollFullPageDown => {
            let visible = state.result_visible_rows();
            if visible == 0 {
                return Some(vec![]);
            }
            let delta = visible.max(1);
            if let Some(row) = state.result_interaction.selection().row() {
                let max_row = result_row_count(state).saturating_sub(1);
                let max_scroll = result_max_scroll(state);
                state
                    .result_interaction
                    .move_row((row + delta).min(max_row));
                state.result_interaction.scroll_offset =
                    (state.result_interaction.scroll_offset + delta).min(max_scroll);
            } else {
                let max = result_max_scroll(state);
                state.result_interaction.scroll_offset =
                    (state.result_interaction.scroll_offset + delta).min(max);
            }
            Some(vec![])
        }
        Action::ResultScrollFullPageUp => {
            let visible = state.result_visible_rows();
            if visible == 0 {
                return Some(vec![]);
            }
            let delta = visible.max(1);
            if let Some(row) = state.result_interaction.selection().row() {
                state.result_interaction.move_row(row.saturating_sub(delta));
                state.result_interaction.scroll_offset =
                    state.result_interaction.scroll_offset.saturating_sub(delta);
            } else {
                state.result_interaction.scroll_offset =
                    state.result_interaction.scroll_offset.saturating_sub(delta);
            }
            Some(vec![])
        }
        // Scroll-to-cursor (zz/zt/zb): only meaningful in RowActive/CellActive
        Action::ResultScrollCursorCenter => {
            state.ui.pending_z = false;
            if let Some(row) = state.result_interaction.selection().row() {
                let visible = state.result_visible_rows();
                if visible > 0 {
                    let max_scroll = result_max_scroll(state);
                    state.result_interaction.scroll_offset =
                        row.saturating_sub(visible / 2).min(max_scroll);
                }
            }
            Some(vec![])
        }
        Action::ResultScrollCursorTop => {
            state.ui.pending_z = false;
            if let Some(row) = state.result_interaction.selection().row() {
                let visible = state.result_visible_rows();
                if visible > 0 {
                    let max_scroll = result_max_scroll(state);
                    state.result_interaction.scroll_offset = row.min(max_scroll);
                }
            }
            Some(vec![])
        }
        Action::ResultScrollCursorBottom => {
            state.ui.pending_z = false;
            if let Some(row) = state.result_interaction.selection().row() {
                let visible = state.result_visible_rows();
                if visible > 0 {
                    let max_scroll = result_max_scroll(state);
                    state.result_interaction.scroll_offset = row
                        .saturating_sub(visible.saturating_sub(1))
                        .min(max_scroll);
                }
            }
            Some(vec![])
        }
        Action::ResultScrollLeft => {
            state.result_interaction.horizontal_offset =
                calculate_prev_column_offset(state.result_interaction.horizontal_offset);
            Some(vec![])
        }
        Action::ResultScrollRight => {
            let plan = &state.ui.result_viewport_plan;
            let all_widths_len = plan.max_offset + plan.column_count;
            state.result_interaction.horizontal_offset = calculate_next_column_offset(
                all_widths_len,
                state.result_interaction.horizontal_offset,
                plan.column_count,
            );
            Some(vec![])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn state_with_result_rows(rows: usize, pane_height: u16) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.ui.result_pane_height = pane_height;
        let result_rows: Vec<Vec<String>> = (0..rows).map(|i| vec![format!("{}", i)]).collect();
        let row_count = result_rows.len();
        state
            .query
            .set_current_result(Arc::new(crate::domain::QueryResult {
                query: String::new(),
                columns: vec!["id".to_string()],
                rows: result_rows,
                row_count,
                execution_time_ms: 1,
                executed_at: std::time::Instant::now(),
                source: crate::domain::QuerySource::Preview,
                error: None,
                command_tag: None,
            }));
        state
    }

    mod result_page_scroll {
        use super::*;

        #[test]
        fn half_page_down_from_top() {
            let mut state = state_with_result_rows(100, 25);
            // visible = 25 - 5 = 20, half = 10
            let effects = reduce(&mut state, &Action::ResultScrollHalfPageDown);

            assert!(effects.is_some());
            assert_eq!(state.result_interaction.scroll_offset, 10);
        }

        #[test]
        fn half_page_up_from_middle() {
            let mut state = state_with_result_rows(100, 25);
            state.result_interaction.scroll_offset = 50;

            reduce(&mut state, &Action::ResultScrollHalfPageUp);

            assert_eq!(state.result_interaction.scroll_offset, 40);
        }

        #[test]
        fn full_page_down_clamped_at_max() {
            let mut state = state_with_result_rows(30, 25);
            // visible = 20, max_scroll = 30-20 = 10
            state.result_interaction.scroll_offset = 5;

            reduce(&mut state, &Action::ResultScrollFullPageDown);

            // delta=20, 5+20=25, clamped to 10
            assert_eq!(state.result_interaction.scroll_offset, 10);
        }

        #[test]
        fn full_page_up_clamped_at_zero() {
            let mut state = state_with_result_rows(100, 25);
            state.result_interaction.scroll_offset = 5;

            reduce(&mut state, &Action::ResultScrollFullPageUp);

            // delta=20, saturating_sub(5,20) = 0
            assert_eq!(state.result_interaction.scroll_offset, 0);
        }

        #[test]
        fn zero_height_pane_scroll_mode_is_noop() {
            let mut state = state_with_result_rows(100, 0);
            // visible = 0 → no-op for all modes
            reduce(&mut state, &Action::ResultScrollHalfPageDown);

            assert_eq!(state.result_interaction.scroll_offset, 0);
        }

        #[test]
        fn scroll_middle_is_noop_in_scroll_mode() {
            let mut state = state_with_result_rows(100, 25);

            reduce(&mut state, &Action::ResultScrollViewportMiddle);

            assert_eq!(state.result_interaction.scroll_offset, 0);
        }

        #[test]
        fn scroll_middle_moves_row_to_viewport_center_in_row_active() {
            let mut state = state_with_result_rows(100, 25);
            // visible = 20, mid_viewport = 10, target = 0 + 10 = 10
            state.result_interaction.enter_row(0);

            reduce(&mut state, &Action::ResultScrollViewportMiddle);

            assert_eq!(state.result_interaction.selection().row(), Some(10));
        }

        #[test]
        fn half_page_down_row_active_moves_both_cursor_and_viewport() {
            let mut state = state_with_result_rows(100, 25);
            // visible=20, half=10
            state.result_interaction.enter_row(10);
            state.result_interaction.scroll_offset = 5;

            reduce(&mut state, &Action::ResultScrollHalfPageDown);

            assert_eq!(state.result_interaction.selection().row(), Some(20));
            assert_eq!(state.result_interaction.scroll_offset, 15);
        }

        #[test]
        fn half_page_up_row_active_moves_both_cursor_and_viewport() {
            let mut state = state_with_result_rows(100, 25);
            // visible=20, half=10
            state.result_interaction.enter_row(30);
            state.result_interaction.scroll_offset = 25;

            reduce(&mut state, &Action::ResultScrollHalfPageUp);

            assert_eq!(state.result_interaction.selection().row(), Some(20));
            assert_eq!(state.result_interaction.scroll_offset, 15);
        }

        #[test]
        fn half_page_down_row_active_preserves_relative_position() {
            let mut state = state_with_result_rows(100, 25);
            // visible=20, half=10
            state.result_interaction.enter_row(15);
            state.result_interaction.scroll_offset = 10;
            // cursor is at relative position 5 within viewport

            reduce(&mut state, &Action::ResultScrollHalfPageDown);

            // both move by 10: cursor=25, offset=20 → relative position still 5
            assert_eq!(state.result_interaction.selection().row(), Some(25));
            assert_eq!(state.result_interaction.scroll_offset, 20);
            let relative = state.result_interaction.selection().row().unwrap()
                - state.result_interaction.scroll_offset;
            assert_eq!(relative, 5);
        }

        #[test]
        fn half_page_down_row_active_clamps_near_bottom() {
            let mut state = state_with_result_rows(100, 25);
            // visible=20, half=10, max_row=99, max_scroll=80
            state.result_interaction.enter_row(95);
            state.result_interaction.scroll_offset = 75;

            reduce(&mut state, &Action::ResultScrollHalfPageDown);

            // cursor: 95+10=105 → clamped to 99
            // scroll: 75+10=85 → clamped to 80
            assert_eq!(state.result_interaction.selection().row(), Some(99));
            assert_eq!(state.result_interaction.scroll_offset, 80);
        }

        #[test]
        fn half_page_up_row_active_clamps_near_top() {
            let mut state = state_with_result_rows(100, 25);
            // visible=20, half=10
            state.result_interaction.enter_row(5);
            state.result_interaction.scroll_offset = 3;

            reduce(&mut state, &Action::ResultScrollHalfPageUp);

            // cursor: 5-10 → 0
            // scroll: 3-10 → 0
            assert_eq!(state.result_interaction.selection().row(), Some(0));
            assert_eq!(state.result_interaction.scroll_offset, 0);
        }

        #[test]
        fn cell_active_half_page_down_preserves_column() {
            let mut state = state_with_result_rows(100, 25);
            state.result_interaction.enter_row(10);
            state.result_interaction.enter_cell(3);
            state.result_interaction.scroll_offset = 5;

            reduce(&mut state, &Action::ResultScrollHalfPageDown);

            assert_eq!(state.result_interaction.selection().row(), Some(20));
            assert_eq!(state.result_interaction.selection().cell(), Some(3));
            assert_eq!(state.result_interaction.scroll_offset, 15);
        }

        #[test]
        fn visible_zero_row_active_is_noop() {
            let mut state = state_with_result_rows(100, 0);
            // visible=0
            state.result_interaction.enter_row(5);
            state.result_interaction.scroll_offset = 3;

            reduce(&mut state, &Action::ResultScrollHalfPageDown);

            assert_eq!(state.result_interaction.selection().row(), Some(5));
            assert_eq!(state.result_interaction.scroll_offset, 3);
        }

        #[test]
        fn data_fewer_than_viewport_scroll_stays_zero() {
            let mut state = state_with_result_rows(10, 25);
            // visible=20, 10 rows < 20 visible → max_scroll=0
            state.result_interaction.enter_row(3);

            reduce(&mut state, &Action::ResultScrollHalfPageDown);

            // cursor: 3+10=13 → clamped to 9
            // scroll: 0+10=10 → clamped to max_scroll=0
            assert_eq!(state.result_interaction.selection().row(), Some(9));
            assert_eq!(state.result_interaction.scroll_offset, 0);
        }

        #[test]
        fn full_page_down_row_active_moves_both() {
            let mut state = state_with_result_rows(100, 25);
            // visible=20, delta=20
            state.result_interaction.enter_row(10);
            state.result_interaction.scroll_offset = 5;

            reduce(&mut state, &Action::ResultScrollFullPageDown);

            assert_eq!(state.result_interaction.selection().row(), Some(30));
            assert_eq!(state.result_interaction.scroll_offset, 25);
        }

        #[test]
        fn full_page_up_row_active_moves_both() {
            let mut state = state_with_result_rows(100, 25);
            // visible=20, delta=20
            state.result_interaction.enter_row(40);
            state.result_interaction.scroll_offset = 30;

            reduce(&mut state, &Action::ResultScrollFullPageUp);

            assert_eq!(state.result_interaction.selection().row(), Some(20));
            assert_eq!(state.result_interaction.scroll_offset, 10);
        }

        #[test]
        fn full_page_down_row_active_clamps_near_bottom() {
            let mut state = state_with_result_rows(100, 25);
            // visible=20, delta=20, max_row=99, max_scroll=80
            state.result_interaction.enter_row(90);
            state.result_interaction.scroll_offset = 75;

            reduce(&mut state, &Action::ResultScrollFullPageDown);

            assert_eq!(state.result_interaction.selection().row(), Some(99));
            assert_eq!(state.result_interaction.scroll_offset, 80);
        }

        #[test]
        fn full_page_up_row_active_clamps_near_top() {
            let mut state = state_with_result_rows(100, 25);
            // visible=20, delta=20
            state.result_interaction.enter_row(10);
            state.result_interaction.scroll_offset = 5;

            reduce(&mut state, &Action::ResultScrollFullPageUp);

            assert_eq!(state.result_interaction.selection().row(), Some(0));
            assert_eq!(state.result_interaction.scroll_offset, 0);
        }

        #[test]
        fn visible_zero_row_active_full_page_is_noop() {
            let mut state = state_with_result_rows(100, 0);
            state.result_interaction.enter_row(5);
            state.result_interaction.scroll_offset = 3;

            reduce(&mut state, &Action::ResultScrollFullPageDown);

            assert_eq!(state.result_interaction.selection().row(), Some(5));
            assert_eq!(state.result_interaction.scroll_offset, 3);
        }
    }

    mod result_scroll_to_cursor {
        use super::*;

        #[test]
        fn scroll_cursor_center_centers_on_selected_row() {
            let mut state = state_with_result_rows(100, 25);
            // visible = 20
            state.result_interaction.enter_row(50);
            state.result_interaction.scroll_offset = 50;
            state.ui.pending_z = true;

            reduce(&mut state, &Action::ResultScrollCursorCenter);

            // row=50, visible=20, offset=50-10=40, max=80 → 40
            assert_eq!(state.result_interaction.scroll_offset, 40);
            assert!(!state.ui.pending_z);
        }

        #[test]
        fn scroll_cursor_top_puts_row_at_top() {
            let mut state = state_with_result_rows(100, 25);
            state.result_interaction.enter_row(30);
            state.result_interaction.scroll_offset = 20;
            state.ui.pending_z = true;

            reduce(&mut state, &Action::ResultScrollCursorTop);

            assert_eq!(state.result_interaction.scroll_offset, 30);
            assert!(!state.ui.pending_z);
        }

        #[test]
        fn scroll_cursor_bottom_puts_row_at_bottom() {
            let mut state = state_with_result_rows(100, 25);
            state.result_interaction.enter_row(30);
            state.result_interaction.scroll_offset = 30;
            state.ui.pending_z = true;

            reduce(&mut state, &Action::ResultScrollCursorBottom);

            // row=30, visible=20, offset=30-19=11, max=80 → 11
            assert_eq!(state.result_interaction.scroll_offset, 11);
            assert!(!state.ui.pending_z);
        }

        #[test]
        fn scroll_cursor_center_is_noop_in_scroll_mode() {
            let mut state = state_with_result_rows(100, 25);
            state.result_interaction.scroll_offset = 20;
            state.ui.pending_z = true;

            reduce(&mut state, &Action::ResultScrollCursorCenter);

            // No row selected, offset unchanged
            assert_eq!(state.result_interaction.scroll_offset, 20);
            assert!(!state.ui.pending_z);
        }

        #[test]
        fn scroll_cursor_top_clamps_to_max_scroll() {
            let mut state = state_with_result_rows(100, 25);
            // visible=20, max_scroll=80
            state.result_interaction.enter_row(95);
            state.result_interaction.scroll_offset = 80;
            state.ui.pending_z = true;

            reduce(&mut state, &Action::ResultScrollCursorTop);

            // row=95, clamped to max_scroll=80
            assert_eq!(state.result_interaction.scroll_offset, 80);
            assert!(!state.ui.pending_z);
        }
    }

    mod history_mode_scroll {
        use super::*;

        fn make_result(
            rows: usize,
            source: crate::domain::QuerySource,
        ) -> Arc<crate::domain::QueryResult> {
            let result_rows: Vec<Vec<String>> = (0..rows).map(|i| vec![format!("{}", i)]).collect();
            let row_count = result_rows.len();
            Arc::new(crate::domain::QueryResult {
                query: String::new(),
                columns: vec!["id".to_string()],
                rows: result_rows,
                row_count,
                execution_time_ms: 1,
                executed_at: std::time::Instant::now(),
                source,
                error: None,
                command_tag: None,
            })
        }

        #[test]
        fn page_scroll_uses_history_entry_row_count_not_live_preview() {
            let mut state = AppState::new("test".to_string());
            state.ui.result_pane_height = 25; // visible = 20, half = 10
            // live preview: 100 rows
            state
                .query
                .set_current_result(make_result(100, crate::domain::QuerySource::Preview));
            // history entry: 5 rows
            state
                .query
                .result_history
                .push(make_result(5, crate::domain::QuerySource::Adhoc));
            state.query.enter_history(0);

            state.result_interaction.enter_row(2);
            state.result_interaction.scroll_offset = 0;

            reduce(&mut state, &Action::ResultScrollHalfPageDown);

            // Should clamp to history entry max_row=4, not live preview max_row=99
            assert_eq!(state.result_interaction.selection().row(), Some(4));
            // max_scroll = 5 - 20 = 0 (history has fewer rows than viewport)
            assert_eq!(state.result_interaction.scroll_offset, 0);
        }

        #[test]
        fn row_count_reflects_visible_result_in_history_mode() {
            let mut state = AppState::new("test".to_string());
            state
                .query
                .set_current_result(make_result(100, crate::domain::QuerySource::Preview));
            state
                .query
                .result_history
                .push(make_result(7, crate::domain::QuerySource::Adhoc));
            state.query.enter_history(0);

            assert_eq!(result_row_count(&state), 7);
        }
    }
}
