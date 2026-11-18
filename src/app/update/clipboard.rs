use std::time::{Duration, Instant};

use crate::model::app_state::AppState;
use crate::model::shared::ui_state::YankFlash;
use crate::update::action::{Action, CopyTarget};
use crate::update::dispatch_result::DispatchResult;

const RESULT_YANK_FLASH: Duration = Duration::from_millis(200);

pub(in crate::update) fn reduce_clipboard(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::CopySucceeded(target) => {
            show_copy_feedback(state, *target, now);
            DispatchResult::handled()
        }
        Action::CopyFailed(e) => {
            state.messages.set_error(e.to_string());
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}

fn show_copy_feedback(state: &mut AppState, target: CopyTarget, now: Instant) {
    match target {
        CopyTarget::Panel(flash_id) => state.flash_timers.set(flash_id, now),
        CopyTarget::ResultCell { row, col } => {
            state.result_interaction.set_yank_flash(Some(YankFlash {
                row,
                col: Some(col),
                until: now + RESULT_YANK_FLASH,
            }));
        }
        CopyTarget::ResultRow { row } => {
            state.result_interaction.set_yank_flash(Some(YankFlash {
                row,
                col: None,
                until: now + RESULT_YANK_FLASH,
            }));
        }
        CopyTarget::ConnectionError => state.connection_error.mark_copied_at(now),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::model::shared::flash_timer::FlashId;
    use crate::ports::outbound::clipboard::ClipboardError;

    fn copied(state: &mut AppState, target: CopyTarget, now: Instant) {
        reduce_clipboard(state, &Action::CopySucceeded(target), now);
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

        let flash = state.result_interaction.yank_flash().expect("flash set");
        assert_eq!(flash.row, 1);
        assert_eq!(flash.col, Some(2));
    }

    #[test]
    fn result_row_copy_flashes_the_row() {
        let mut state = AppState::new("test".to_string());

        copied(&mut state, CopyTarget::ResultRow { row: 0 }, Instant::now());

        let flash = state.result_interaction.yank_flash().expect("flash set");
        assert_eq!(flash.row, 0);
        assert_eq!(flash.col, None);
    }

    #[test]
    fn copy_failure_shows_error() {
        let mut state = AppState::new("test".to_string());

        reduce_clipboard(
            &mut state,
            &Action::CopyFailed(ClipboardError::Unavailable("Clipboard unavailable".into())),
            Instant::now(),
        );

        assert_eq!(
            state.messages.last_error.as_deref(),
            Some("Clipboard unavailable")
        );
    }
}
