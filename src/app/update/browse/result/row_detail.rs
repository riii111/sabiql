use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::browse::row_detail::RowDetailState;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::CopyTarget;
use crate::update::action::{Action, ScrollAmount, ScrollDirection};
use crate::update::helpers::clipboard_unavailable;

pub(in crate::update) fn open_modal_row_detail(state: &mut AppState) -> Vec<Effect> {
    let result = match state.query.visible_result() {
        Some(r) if !r.is_error() && r.data_row_count() > 0 => r,
        _ => return vec![],
    };

    let Some(row_idx) = state.result_interaction.selection().row() else {
        return vec![];
    };

    state.row_detail = if result.has_typed_values() {
        let Some(values) = result.values().get(row_idx) else {
            return vec![];
        };
        RowDetailState::open_with_values(&result.columns, values)
    } else {
        let Some(cells) = result.display_row_at(row_idx) else {
            return vec![];
        };
        RowDetailState::open(&result.columns, &cells)
    };
    state.modal.push_mode(InputMode::RowDetail);
    vec![]
}

pub(in crate::update) fn close_modal_row_detail(state: &mut AppState) -> Vec<Effect> {
    state.row_detail.close();
    state.modal.pop_mode();
    vec![]
}

pub(in crate::update) fn row_detail_yank(state: &AppState) -> Vec<Effect> {
    let content = state.row_detail.content_for_yank();
    vec![Effect::CopyToClipboard {
        content,
        on_success: Box::new(Action::CopySucceeded(CopyTarget::Panel(FlashId::RowDetail))),
        on_failure: Some(Box::new(clipboard_unavailable())),
    }]
}

pub(in crate::update) fn row_detail_yank_json(state: &AppState) -> Vec<Effect> {
    let content = state.row_detail.json_for_yank();
    vec![Effect::CopyToClipboard {
        content,
        on_success: Box::new(Action::CopySucceeded(CopyTarget::Panel(FlashId::RowDetail))),
        on_failure: Some(Box::new(clipboard_unavailable())),
    }]
}

pub(in crate::update) fn scroll_row_detail(
    state: &mut AppState,
    direction: ScrollDirection,
    amount: ScrollAmount,
) {
    match (direction, amount) {
        (ScrollDirection::Up, ScrollAmount::Line) => {
            state.row_detail.scroll_up_by(1);
        }
        (ScrollDirection::Down, ScrollAmount::Line) => {
            state
                .row_detail
                .scroll_down_by(1, state.row_detail_content_visible_rows());
        }
        (ScrollDirection::Up, ScrollAmount::ToStart) => {
            state.row_detail.scroll_to_start();
        }
        (ScrollDirection::Down, ScrollAmount::ToEnd) => {
            state
                .row_detail
                .scroll_to_end(state.row_detail_content_visible_rows());
        }
        (ScrollDirection::Left, ScrollAmount::Line) => {
            state.row_detail.scroll_left_by(1);
        }
        (ScrollDirection::Right, ScrollAmount::Line) => {
            state
                .row_detail
                .scroll_right_by(1, state.row_detail_content_visible_columns());
        }
        (_, ScrollAmount::HalfPage | ScrollAmount::FullPage) => {
            let visible = state.row_detail_content_visible_rows();
            if let Some(delta) = amount.page_delta(visible) {
                match direction {
                    ScrollDirection::Up => state.row_detail.scroll_up_by(delta),
                    ScrollDirection::Down => state.row_detail.scroll_down_by(delta, visible),
                    ScrollDirection::Left | ScrollDirection::Right => {}
                }
            }
        }
        _ => debug_assert!(
            false,
            "unsupported RowDetail scroll: {direction:?} {amount:?}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    use crate::domain::{QueryResult, QuerySource, QueryValue};
    use crate::update::action::{ModalKind, ScrollTarget};
    use crate::update::reducer::route;

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
        route(
            &mut state,
            Action::OpenModal(ModalKind::RowDetail),
            Instant::now(),
        );
        state
    }

    #[test]
    fn open_builds_row_detail() {
        let mut state = state_with_result();

        route(
            &mut state,
            Action::OpenModal(ModalKind::RowDetail),
            Instant::now(),
        );

        assert!(state.row_detail.is_active());
        assert_eq!(state.input_mode(), InputMode::RowDetail);
        assert!(state.row_detail.content().contains("id\n  1"));
        assert!(state.row_detail.content().contains("name\n  alice"));
        assert!(state.row_detail.json_for_yank().contains("\"id\": 1"));
        assert!(
            state
                .row_detail
                .json_for_yank()
                .contains("\"name\": \"alice\"")
        );
    }

    #[test]
    fn open_builds_row_detail_from_typed_values() {
        let mut state = AppState::new("test".to_string());
        state
            .query
            .set_current_result(Arc::new(QueryResult::success_with_values(
                "SELECT payload".to_string(),
                vec!["payload".to_string()],
                vec![vec![QueryValue::Blob(vec![0xAB, 0xCD])]],
                1,
                QuerySource::Preview,
            )));
        state.result_interaction.activate_cell(0, 0);

        route(
            &mut state,
            Action::OpenModal(ModalKind::RowDetail),
            Instant::now(),
        );

        assert!(state.row_detail.content().contains("BLOB (2 bytes) AB CD"));
        assert!(state.row_detail.json_for_yank().contains("X'ABCD'"));
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

        route(
            &mut state,
            Action::OpenModal(ModalKind::RowDetail),
            Instant::now(),
        );

        assert!(!state.row_detail.is_active());
        assert_eq!(state.input_mode(), InputMode::Normal);
    }

    #[test]
    fn close_clears_state() {
        let mut state = state_with_result();
        route(
            &mut state,
            Action::OpenModal(ModalKind::RowDetail),
            Instant::now(),
        );
        assert!(state.row_detail.is_active());

        route(
            &mut state,
            Action::CloseModal(ModalKind::RowDetail),
            Instant::now(),
        );

        assert!(!state.row_detail.is_active());
        assert_eq!(state.input_mode(), InputMode::Normal);
    }

    #[test]
    fn yank_display_text_returns_clipboard_effect() {
        let mut state = state_with_row_detail();

        let effects = route(&mut state, Action::RowDetailYank, Instant::now());

        assert_eq!(effects.len(), 1);
        assert!(matches!(
            &effects[0],
            Effect::CopyToClipboard { content, on_success, .. }
            if content.contains("id\n  1") && matches!(on_success.as_ref(), Action::CopySucceeded(CopyTarget::Panel(FlashId::RowDetail)))
        ));
    }

    #[test]
    fn yank_json_returns_clipboard_effect() {
        let mut state = state_with_row_detail();

        let effects = route(&mut state, Action::RowDetailYankJson, Instant::now());

        assert_eq!(effects.len(), 1);
        assert!(matches!(
            &effects[0],
            Effect::CopyToClipboard { content, on_success, .. }
            if content.contains("\"name\": \"alice\"") && matches!(on_success.as_ref(), Action::CopySucceeded(CopyTarget::Panel(FlashId::RowDetail)))
        ));
    }

    #[test]
    fn scroll_down_clamps_to_bottom_of_viewport() {
        let mut state = state_with_row_detail();
        state.ui.row_detail_content_visible_rows = 3;
        let line_count = state.row_detail.line_count();
        assert!(line_count > 3, "test content should span more than 3 lines");
        state.row_detail.scroll_down_by(line_count - 2, 1);

        route(
            &mut state,
            Action::Scroll {
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

        route(
            &mut state,
            Action::Scroll {
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
    fn horizontal_scroll_right_clamps_to_content_width() {
        let mut state = AppState::new("test".to_string());
        state
            .query
            .set_current_result(Arc::new(QueryResult::success(
                "SELECT * FROM users".to_string(),
                vec!["note".to_string()],
                vec![vec!["abcdefghijklmnopqrstuvwxyz".to_string()]],
                1,
                QuerySource::Preview,
            )));
        state.result_interaction.activate_cell(0, 0);
        route(
            &mut state,
            Action::OpenModal(ModalKind::RowDetail),
            Instant::now(),
        );
        state.ui.row_detail_content_visible_columns = 10;

        for _ in 0..100 {
            route(
                &mut state,
                Action::Scroll {
                    target: ScrollTarget::RowDetail,
                    direction: ScrollDirection::Right,
                    amount: ScrollAmount::Line,
                },
                Instant::now(),
            );
        }

        assert_eq!(
            state.row_detail.horizontal_offset(),
            state.row_detail.content_width().saturating_sub(10)
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

        route(
            &mut state,
            Action::OpenModal(ModalKind::RowDetail),
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

        route(
            &mut state,
            Action::OpenModal(ModalKind::RowDetail),
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

        route(
            &mut state,
            Action::Scroll {
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
        state
            .row_detail
            .scroll_down_by(line_count.saturating_sub(3), 3);

        route(
            &mut state,
            Action::Scroll {
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
        state.row_detail.scroll_down_by(max_scroll, 3);

        route(
            &mut state,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::HalfPage,
            },
            Instant::now(),
        );

        assert_eq!(state.row_detail.scroll_offset(), max_scroll);
    }

    #[test]
    fn scroll_half_page_down_moves_by_half_visible_rows() {
        let mut state = state_with_row_detail();
        state.ui.row_detail_content_visible_rows = 2;

        route(
            &mut state,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::HalfPage,
            },
            Instant::now(),
        );

        assert_eq!(state.row_detail.scroll_offset(), 1);
    }

    #[test]
    fn scroll_half_page_up_from_bottom_stops_at_top() {
        let mut state = state_with_row_detail();
        // Make the half-page delta (visible / 2 = 5) larger than the starting
        // offset so the test actually exercises saturating_sub clamping.
        state.row_detail.scroll_down_by(2, 1);
        state.ui.row_detail_content_visible_rows = 10;

        route(
            &mut state,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::HalfPage,
            },
            Instant::now(),
        );

        assert_eq!(state.row_detail.scroll_offset(), 0);
    }
}
