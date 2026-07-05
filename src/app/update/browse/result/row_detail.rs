use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::browse::row_detail::RowDetailState;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::input_mode::InputMode;
use crate::ports::outbound::ClipboardError;
use crate::update::action::{Action, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget};
use crate::update::dispatch_result::DispatchResult;

pub fn reduce_row_detail(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::RowDetail) => {
            let result = match state.query.visible_result() {
                Some(r) if !r.is_error() && !r.rows.is_empty() => r,
                _ => return DispatchResult::handled(),
            };

            let Some(row_idx) = state.result_interaction.selection().row() else {
                return DispatchResult::handled();
            };

            let Some(cells) = result.rows.get(row_idx) else {
                return DispatchResult::handled();
            };

            state.row_detail = RowDetailState::open(row_idx, &result.columns, cells);
            state.modal.push_mode(InputMode::RowDetail);
            DispatchResult::handled()
        }

        Action::CloseModal(ModalKind::RowDetail) => {
            state.row_detail.close();
            state.modal.pop_mode();
            DispatchResult::handled()
        }

        Action::RowDetailYank => {
            let content = state.row_detail.content_for_yank();
            DispatchResult::handled_with(vec![Effect::CopyToClipboard {
                content,
                on_success: Some(Box::new(Action::RowDetailYankSuccess)),
                on_failure: Some(Box::new(Action::CopyFailed(ClipboardError::Unavailable(
                    "Clipboard unavailable".into(),
                )))),
            }])
        }

        Action::RowDetailYankSuccess => {
            state.flash_timers.set(FlashId::RowDetail, now);
            DispatchResult::handled()
        }

        Action::Scroll {
            target: ScrollTarget::RowDetail,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        } => {
            *state.row_detail.scroll_offset_mut() =
                state.row_detail.scroll_offset().saturating_sub(1);
            DispatchResult::handled()
        }

        Action::Scroll {
            target: ScrollTarget::RowDetail,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        } => {
            let max_scroll = state
                .row_detail
                .line_count()
                .saturating_sub(state.row_detail_content_visible_rows().max(1));
            *state.row_detail.scroll_offset_mut() =
                (*state.row_detail.scroll_offset_mut() + 1).min(max_scroll);
            DispatchResult::handled()
        }

        Action::Scroll {
            target: ScrollTarget::RowDetail,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::ToStart,
        } => {
            *state.row_detail.scroll_offset_mut() = 0;
            DispatchResult::handled()
        }

        Action::Scroll {
            target: ScrollTarget::RowDetail,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::ToEnd,
        } => {
            *state.row_detail.scroll_offset_mut() = state
                .row_detail
                .line_count()
                .saturating_sub(state.row_detail_content_visible_rows().max(1));
            DispatchResult::handled()
        }

        Action::Scroll {
            target: ScrollTarget::RowDetail,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::FullPage,
        } => {
            let visible = state.row_detail_content_visible_rows().max(1);
            let max_scroll = state.row_detail.line_count().saturating_sub(visible);
            *state.row_detail.scroll_offset_mut() =
                (*state.row_detail.scroll_offset_mut() + visible).min(max_scroll);
            DispatchResult::handled()
        }

        Action::Scroll {
            target: ScrollTarget::RowDetail,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::FullPage,
        } => {
            let visible = state.row_detail_content_visible_rows().max(1);
            *state.row_detail.scroll_offset_mut() =
                state.row_detail.scroll_offset().saturating_sub(visible);
            DispatchResult::handled()
        }

        Action::Scroll {
            target: ScrollTarget::RowDetail,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::HalfPage,
        } => {
            let visible = state.row_detail_content_visible_rows().max(1);
            let max_scroll = state.row_detail.line_count().saturating_sub(visible);
            *state.row_detail.scroll_offset_mut() =
                (*state.row_detail.scroll_offset_mut() + visible / 2).min(max_scroll);
            DispatchResult::handled()
        }

        Action::Scroll {
            target: ScrollTarget::RowDetail,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::HalfPage,
        } => {
            let visible = state.row_detail_content_visible_rows().max(1);
            *state.row_detail.scroll_offset_mut() =
                state.row_detail.scroll_offset().saturating_sub(visible / 2);
            DispatchResult::handled()
        }

        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::domain::{QueryResult, QuerySource};

    fn state_with_result() -> AppState {
        let mut state = AppState::new("test".to_string());
        state
            .query
            .set_current_result(Arc::new(QueryResult::success(
                "SELECT * FROM users".to_string(),
                vec!["id".to_string(), "name".to_string()],
                vec![vec!["1".to_string(), "alice".to_string()]],
                1,
                QuerySource::Preview,
            )));
        state.result_interaction.activate_cell(0, 0);
        state
    }

    fn state_with_row_detail() -> AppState {
        let mut state = state_with_result();
        reduce_row_detail(
            &mut state,
            &Action::OpenModal(ModalKind::RowDetail),
            Instant::now(),
        );
        state
    }

    #[test]
    fn open_builds_row_detail() {
        let mut state = state_with_result();

        reduce_row_detail(
            &mut state,
            &Action::OpenModal(ModalKind::RowDetail),
            Instant::now(),
        );

        assert!(state.row_detail.is_active());
        assert_eq!(state.input_mode(), InputMode::RowDetail);
        assert!(state.row_detail.content().contains("id\n  1"));
        assert!(state.row_detail.content().contains("name\n  alice"));
        assert!(state.row_detail.content_for_yank().contains("\"id\": 1"));
        assert!(
            state
                .row_detail
                .content_for_yank()
                .contains("\"name\": \"alice\"")
        );
    }

    #[test]
    fn open_without_selection_is_noop() {
        let mut state = AppState::new("test".to_string());
        state
            .query
            .set_current_result(Arc::new(QueryResult::success(
                "SELECT 1".to_string(),
                vec!["id".to_string()],
                vec![vec!["1".to_string()]],
                1,
                QuerySource::Preview,
            )));

        reduce_row_detail(
            &mut state,
            &Action::OpenModal(ModalKind::RowDetail),
            Instant::now(),
        );

        assert!(!state.row_detail.is_active());
        assert_eq!(state.input_mode(), InputMode::Normal);
    }

    #[test]
    fn close_clears_state() {
        let mut state = state_with_result();
        reduce_row_detail(
            &mut state,
            &Action::OpenModal(ModalKind::RowDetail),
            Instant::now(),
        );
        assert!(state.row_detail.is_active());

        reduce_row_detail(
            &mut state,
            &Action::CloseModal(ModalKind::RowDetail),
            Instant::now(),
        );

        assert!(!state.row_detail.is_active());
        assert_eq!(state.input_mode(), InputMode::Normal);
    }

    #[test]
    fn yank_returns_clipboard_effect() {
        let mut state = state_with_row_detail();

        let effects = reduce_row_detail(&mut state, &Action::RowDetailYank, Instant::now())
            .into_effects()
            .expect("should return effects");

        assert_eq!(effects.len(), 1);
        assert!(matches!(
            &effects[0],
            Effect::CopyToClipboard { content, on_success, .. }
            if content.contains("alice") && matches!(on_success.as_deref(), Some(Action::RowDetailYankSuccess))
        ));
    }

    #[test]
    fn scroll_down_clamps_to_bottom_of_viewport() {
        let mut state = state_with_row_detail();
        state.ui.row_detail_content_visible_rows = 3;
        let line_count = state.row_detail.line_count();
        assert!(line_count > 3, "test content should span more than 3 lines");
        *state.row_detail.scroll_offset_mut() = line_count - 2;

        reduce_row_detail(
            &mut state,
            &Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            },
            Instant::now(),
        );

        assert_eq!(
            state.row_detail.scroll_offset(),
            line_count.saturating_sub(3)
        );
    }

    #[test]
    fn scroll_to_end_clamps_to_bottom_of_viewport() {
        let mut state = state_with_row_detail();
        state.ui.row_detail_content_visible_rows = 3;
        let line_count = state.row_detail.line_count();
        assert!(line_count > 3, "test content should span more than 3 lines");

        reduce_row_detail(
            &mut state,
            &Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::ToEnd,
            },
            Instant::now(),
        );

        assert_eq!(
            state.row_detail.scroll_offset(),
            line_count.saturating_sub(3)
        );
    }

    #[test]
    fn open_on_error_result_is_noop() {
        let mut state = AppState::new("test".to_string());
        state.query.set_current_result(Arc::new(QueryResult::error(
            "SELECT 1".to_string(),
            "boom".to_string(),
            0,
            QuerySource::Preview,
        )));

        reduce_row_detail(
            &mut state,
            &Action::OpenModal(ModalKind::RowDetail),
            Instant::now(),
        );

        assert!(!state.row_detail.is_active());
        assert_eq!(state.input_mode(), InputMode::Normal);
    }

    #[test]
    fn open_on_empty_rows_is_noop() {
        let mut state = AppState::new("test".to_string());
        state
            .query
            .set_current_result(Arc::new(QueryResult::success(
                "SELECT 1".to_string(),
                vec!["id".to_string()],
                vec![],
                0,
                QuerySource::Preview,
            )));

        reduce_row_detail(
            &mut state,
            &Action::OpenModal(ModalKind::RowDetail),
            Instant::now(),
        );

        assert!(!state.row_detail.is_active());
        assert_eq!(state.input_mode(), InputMode::Normal);
    }

    #[test]
    fn scroll_full_page_down_clamps_to_bottom() {
        let mut state = state_with_row_detail();
        state.ui.row_detail_content_visible_rows = 3;
        let line_count = state.row_detail.line_count();
        assert!(line_count > 3);

        reduce_row_detail(
            &mut state,
            &Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::FullPage,
            },
            Instant::now(),
        );

        assert_eq!(
            state.row_detail.scroll_offset(),
            line_count.saturating_sub(3)
        );
    }

    #[test]
    fn scroll_full_page_up_from_bottom_stops_at_top() {
        let mut state = state_with_row_detail();
        state.ui.row_detail_content_visible_rows = 3;
        let line_count = state.row_detail.line_count();
        *state.row_detail.scroll_offset_mut() = line_count.saturating_sub(3);

        reduce_row_detail(
            &mut state,
            &Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::FullPage,
            },
            Instant::now(),
        );

        assert_eq!(state.row_detail.scroll_offset(), 0);
    }

    #[test]
    fn scroll_half_page_down_clamps_to_bottom() {
        let mut state = state_with_row_detail();
        state.ui.row_detail_content_visible_rows = 3;
        let line_count = state.row_detail.line_count();
        let max_scroll = line_count.saturating_sub(3);
        *state.row_detail.scroll_offset_mut() = max_scroll;

        reduce_row_detail(
            &mut state,
            &Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::HalfPage,
            },
            Instant::now(),
        );

        assert_eq!(state.row_detail.scroll_offset(), max_scroll);
    }

    #[test]
    fn scroll_half_page_up_from_bottom_stops_at_top() {
        let mut state = state_with_row_detail();
        // Make the half-page delta (visible / 2 = 5) larger than the starting
        // offset so the test actually exercises saturating_sub clamping.
        state.ui.row_detail_content_visible_rows = 10;
        *state.row_detail.scroll_offset_mut() = 2;

        reduce_row_detail(
            &mut state,
            &Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::HalfPage,
            },
            Instant::now(),
        );

        assert_eq!(state.row_detail.scroll_offset(), 0);
    }
}
