use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::state::AppState;

fn reset_result_view(state: &mut AppState) {
    state.ui.result_scroll_offset = 0;
    state.ui.result_horizontal_offset = 0;
    state.ui.result_selection.reset();
    state.cell_edit.clear();
    state.ui.staged_delete_rows.clear();
    state.pending_write_preview = None;
}

pub fn reduce(state: &mut AppState, action: &Action) -> Option<Vec<Effect>> {
    match action {
        Action::OpenResultHistory => {
            let len = state.query.result_history.len();
            if len == 0 {
                return Some(vec![]);
            }
            state.query.history_index = Some(len - 1);
            reset_result_view(state);
            Some(vec![])
        }
        Action::HistoryOlder => {
            if let Some(idx) = state.query.history_index
                && idx > 0
            {
                state.query.history_index = Some(idx - 1);
                reset_result_view(state);
            }
            Some(vec![])
        }
        Action::HistoryNewer => {
            if let Some(idx) = state.query.history_index {
                let len = state.query.result_history.len();
                if idx + 1 < len {
                    state.query.history_index = Some(idx + 1);
                    reset_result_view(state);
                }
            }
            Some(vec![])
        }
        Action::ExitResultHistory => {
            state.query.history_index = None;
            reset_result_view(state);
            Some(vec![])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{QueryResult, QuerySource};
    use std::sync::Arc;

    fn make_result(query: &str) -> Arc<QueryResult> {
        Arc::new(QueryResult::success(
            query.to_string(),
            vec!["col".to_string()],
            vec![vec!["val".to_string()]],
            10,
            QuerySource::Adhoc,
        ))
    }

    fn state_with_history(count: usize) -> AppState {
        let mut state = AppState::new("test".to_string());
        for i in 0..count {
            state
                .query
                .result_history
                .push(make_result(&format!("SELECT {}", i + 1)));
        }
        state.query.current_result = Some(make_result("SELECT latest"));
        state
    }

    #[test]
    fn open_sets_index_to_newest() {
        let mut state = state_with_history(3);

        reduce(&mut state, &Action::OpenResultHistory);

        assert_eq!(state.query.history_index, Some(2));
    }

    #[test]
    fn open_is_noop_when_history_empty() {
        let mut state = AppState::new("test".to_string());

        reduce(&mut state, &Action::OpenResultHistory);

        assert_eq!(state.query.history_index, None);
    }

    #[test]
    fn older_decrements_index() {
        let mut state = state_with_history(3);
        state.query.history_index = Some(2);

        reduce(&mut state, &Action::HistoryOlder);

        assert_eq!(state.query.history_index, Some(1));
    }

    #[test]
    fn older_clamps_at_zero() {
        let mut state = state_with_history(3);
        state.query.history_index = Some(0);

        reduce(&mut state, &Action::HistoryOlder);

        assert_eq!(state.query.history_index, Some(0));
    }

    #[test]
    fn newer_increments_index() {
        let mut state = state_with_history(3);
        state.query.history_index = Some(0);

        reduce(&mut state, &Action::HistoryNewer);

        assert_eq!(state.query.history_index, Some(1));
    }

    #[test]
    fn newer_at_last_is_noop() {
        let mut state = state_with_history(3);
        state.query.history_index = Some(2);

        reduce(&mut state, &Action::HistoryNewer);

        assert_eq!(state.query.history_index, Some(2));
    }

    #[test]
    fn exit_clears_index() {
        let mut state = state_with_history(3);
        state.query.history_index = Some(1);

        reduce(&mut state, &Action::ExitResultHistory);

        assert_eq!(state.query.history_index, None);
    }

    #[test]
    fn navigation_resets_scroll_offset() {
        let mut state = state_with_history(3);
        state.ui.result_scroll_offset = 10;
        state.ui.result_horizontal_offset = 5;

        reduce(&mut state, &Action::OpenResultHistory);

        assert_eq!(state.ui.result_scroll_offset, 0);
        assert_eq!(state.ui.result_horizontal_offset, 0);
    }
}
