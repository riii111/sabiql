use std::time::Instant;

use crate::model::app_state::AppState;
use crate::ports::outbound::clipboard::ClipboardError;
use crate::update::action::CopyTarget;

pub(in crate::update) fn copied(
    state: &mut AppState,
    target: CopyTarget,
    outcome: Result<(), ClipboardError>,
    now: Instant,
) {
    if let Err(error) = outcome {
        let message = if target == CopyTarget::ConnectionError {
            error.to_string()
        } else {
            "Clipboard unavailable".to_string()
        };
        state.messages.set_error(message);
        return;
    }
    match target {
        CopyTarget::Panel(flash_id) => state.flash_timers.set(flash_id, now),
        CopyTarget::ResultCell { row, col } => state.flash_timers.set_result(row, Some(col), now),
        CopyTarget::ResultRow { row } => state.flash_timers.set_result(row, None, now),
        CopyTarget::ConnectionError => state.connection_error.mark_copied_at(now),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::model::shared::flash_timer::FlashId;
    use crate::update::action::Action;
    use crate::update::reducer::route;

    fn copied(state: &mut AppState, target: CopyTarget, now: Instant) {
        route(
            state,
            Action::Copied {
                target,
                outcome: Ok(()),
            },
            now,
        );
    }

    #[rstest]
    #[case(FlashId::SqlModal)]
    #[case(FlashId::Ddl)]
    #[case(FlashId::JsonDetail)]
    #[case(FlashId::CellDetail)]
    #[case(FlashId::RowDetail)]
    fn panel_copy_sets_its_flash(#[case] flash_id: FlashId) {
        let mut state = AppState::new("test".to_string());
        let now = Instant::now();

        copied(&mut state, CopyTarget::Panel(flash_id), now);

        assert!(state.flash_timers.is_active(flash_id, now));
    }

    #[test]
    fn result_cell_copy_flashes_the_cell() {
        let mut state = AppState::new("test".to_string());

        copied(
            &mut state,
            CopyTarget::ResultCell { row: 1, col: 2 },
            Instant::now(),
        );

        let flash = state.flash_timers.result().expect("flash set");
        assert_eq!(flash.row, 1);
        assert_eq!(flash.col, Some(2));
    }

    #[test]
    fn result_row_copy_flashes_the_row() {
        let mut state = AppState::new("test".to_string());

        copied(&mut state, CopyTarget::ResultRow { row: 0 }, Instant::now());

        let flash = state.flash_timers.result().expect("flash set");
        assert_eq!(flash.row, 0);
        assert_eq!(flash.col, None);
    }

    #[rstest]
    #[case(CopyTarget::ResultRow { row: 0 }, "Clipboard unavailable")]
    #[case(CopyTarget::ConnectionError, "no display")]
    fn copy_failure_shows_error_without_feedback(
        #[case] target: CopyTarget,
        #[case] expected: &str,
    ) {
        let mut state = AppState::new("test".to_string());

        route(
            &mut state,
            Action::Copied {
                target,
                outcome: Err(ClipboardError::Unavailable("no display".into())),
            },
            Instant::now(),
        );

        assert_eq!(state.messages.last_error.as_deref(), Some(expected));
        assert!(state.flash_timers.result().is_none());
    }
}
