use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::state::AppState;
use crate::app::viewport::{calculate_next_column_offset, calculate_prev_column_offset};

pub(super) fn result_row_count(state: &AppState) -> usize {
    state
        .query
        .current_result
        .as_ref()
        .map(|r| r.rows.len())
        .unwrap_or(0)
}

pub(super) fn result_col_count(state: &AppState) -> usize {
    state
        .query
        .current_result
        .as_ref()
        .map(|r| r.columns.len())
        .unwrap_or(0)
}

pub(super) fn result_max_scroll(state: &AppState) -> usize {
    let visible = state.result_visible_rows();
    result_row_count(state).saturating_sub(visible)
}

fn ensure_row_visible(state: &mut AppState) {
    if let Some(row) = state.ui.result_selection.row() {
        let visible = state.result_visible_rows();
        if visible == 0 {
            return;
        }
        if row < state.ui.result_scroll_offset {
            state.ui.result_scroll_offset = row;
        } else if row >= state.ui.result_scroll_offset + visible {
            state.ui.result_scroll_offset = row - visible + 1;
        }
    }
}

fn move_row_or_scroll(state: &mut AppState, new_row: usize, scroll_fn: impl FnOnce(&mut AppState)) {
    if state.ui.result_selection.row().is_some() {
        state.ui.result_selection.move_row(new_row);
        ensure_row_visible(state);
    } else {
        scroll_fn(state);
    }
}

pub fn reduce(state: &mut AppState, action: &Action) -> Option<Vec<Effect>> {
    match action {
        Action::ResultScrollUp => {
            let new_row = state
                .ui
                .result_selection
                .row()
                .and_then(|r| r.checked_sub(1));
            match new_row {
                Some(r) => move_row_or_scroll(state, r, |_| {}),
                None if state.ui.result_selection.row().is_none() => {
                    state.ui.result_scroll_offset = state.ui.result_scroll_offset.saturating_sub(1);
                }
                _ => {} // row == 0, no-op
            }
            Some(vec![])
        }
        Action::ResultScrollDown => {
            let max_row = result_row_count(state).saturating_sub(1);
            let new_row = state
                .ui
                .result_selection
                .row()
                .map(|r| (r + 1).min(max_row))
                .unwrap_or(0);
            move_row_or_scroll(state, new_row, |s| {
                let max_scroll = result_max_scroll(s);
                if s.ui.result_scroll_offset < max_scroll {
                    s.ui.result_scroll_offset += 1;
                }
            });
            Some(vec![])
        }
        Action::ResultScrollTop => {
            move_row_or_scroll(state, 0, |s| s.ui.result_scroll_offset = 0);
            Some(vec![])
        }
        Action::ResultScrollBottom => {
            let max_row = result_row_count(state).saturating_sub(1);
            let max_scroll = result_max_scroll(state);
            move_row_or_scroll(state, max_row, |s| s.ui.result_scroll_offset = max_scroll);
            Some(vec![])
        }
        Action::ResultScrollHalfPageDown => {
            let delta = (state.result_visible_rows() / 2).max(1);
            let max_row = result_row_count(state).saturating_sub(1);
            let new_row = state
                .ui
                .result_selection
                .row()
                .map(|r| (r + delta).min(max_row))
                .unwrap_or(0);
            move_row_or_scroll(state, new_row, |s| {
                let max = result_max_scroll(s);
                s.ui.result_scroll_offset = (s.ui.result_scroll_offset + delta).min(max);
            });
            Some(vec![])
        }
        Action::ResultScrollHalfPageUp => {
            let delta = (state.result_visible_rows() / 2).max(1);
            let new_row = state
                .ui
                .result_selection
                .row()
                .map(|r| r.saturating_sub(delta))
                .unwrap_or(0);
            move_row_or_scroll(state, new_row, |s| {
                s.ui.result_scroll_offset = s.ui.result_scroll_offset.saturating_sub(delta);
            });
            Some(vec![])
        }
        Action::ResultScrollFullPageDown => {
            let delta = state.result_visible_rows().max(1);
            let max_row = result_row_count(state).saturating_sub(1);
            let new_row = state
                .ui
                .result_selection
                .row()
                .map(|r| (r + delta).min(max_row))
                .unwrap_or(0);
            move_row_or_scroll(state, new_row, |s| {
                let max = result_max_scroll(s);
                s.ui.result_scroll_offset = (s.ui.result_scroll_offset + delta).min(max);
            });
            Some(vec![])
        }
        Action::ResultScrollFullPageUp => {
            let delta = state.result_visible_rows().max(1);
            let new_row = state
                .ui
                .result_selection
                .row()
                .map(|r| r.saturating_sub(delta))
                .unwrap_or(0);
            move_row_or_scroll(state, new_row, |s| {
                s.ui.result_scroll_offset = s.ui.result_scroll_offset.saturating_sub(delta);
            });
            Some(vec![])
        }
        Action::ResultScrollLeft => {
            state.ui.result_horizontal_offset =
                calculate_prev_column_offset(state.ui.result_horizontal_offset);
            Some(vec![])
        }
        Action::ResultScrollRight => {
            let plan = &state.ui.result_viewport_plan;
            let all_widths_len = plan.max_offset + plan.column_count;
            state.ui.result_horizontal_offset = calculate_next_column_offset(
                all_widths_len,
                state.ui.result_horizontal_offset,
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

    mod result_page_scroll {
        use super::*;

        fn state_with_result_rows(rows: usize, pane_height: u16) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.result_pane_height = pane_height;
            let result_rows: Vec<Vec<String>> = (0..rows).map(|i| vec![format!("{}", i)]).collect();
            let row_count = result_rows.len();
            state.query.current_result = Some(Arc::new(crate::domain::QueryResult {
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

        #[test]
        fn half_page_down_from_top() {
            let mut state = state_with_result_rows(100, 25);
            // visible = 25 - 5 = 20, half = 10
            let effects = reduce(&mut state, &Action::ResultScrollHalfPageDown);

            assert!(effects.is_some());
            assert_eq!(state.ui.result_scroll_offset, 10);
        }

        #[test]
        fn half_page_up_from_middle() {
            let mut state = state_with_result_rows(100, 25);
            state.ui.result_scroll_offset = 50;

            reduce(&mut state, &Action::ResultScrollHalfPageUp);

            assert_eq!(state.ui.result_scroll_offset, 40);
        }

        #[test]
        fn full_page_down_clamped_at_max() {
            let mut state = state_with_result_rows(30, 25);
            // visible = 20, max_scroll = 30-20 = 10
            state.ui.result_scroll_offset = 5;

            reduce(&mut state, &Action::ResultScrollFullPageDown);

            // delta=20, 5+20=25, clamped to 10
            assert_eq!(state.ui.result_scroll_offset, 10);
        }

        #[test]
        fn full_page_up_clamped_at_zero() {
            let mut state = state_with_result_rows(100, 25);
            state.ui.result_scroll_offset = 5;

            reduce(&mut state, &Action::ResultScrollFullPageUp);

            // delta=20, saturating_sub(5,20) = 0
            assert_eq!(state.ui.result_scroll_offset, 0);
        }

        #[test]
        fn zero_height_pane_scrolls_by_one() {
            let mut state = state_with_result_rows(100, 0);
            // visible = 0, delta = max(0/2,1) = 1
            reduce(&mut state, &Action::ResultScrollHalfPageDown);

            assert_eq!(state.ui.result_scroll_offset, 1);
        }
    }
}
