use std::time::Instant;

use crate::model::app_state::AppState;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, ModalKind};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_base_lifecycle(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::TablePicker) => {
            state.modal.set_mode(InputMode::TablePicker);
            state.ui.table_picker.clear_filter_and_reset();
            DispatchResult::handled()
        }
        Action::CloseModal(ModalKind::TablePicker)
        | Action::CloseModal(ModalKind::CommandPalette) => {
            state.modal.set_mode(InputMode::Normal);
            DispatchResult::handled()
        }
        Action::Escape => match state.modal.active_mode() {
            InputMode::Normal | InputMode::ConnectionSelector => {
                state.modal.set_mode(InputMode::Normal);
                DispatchResult::handled()
            }
            _ => DispatchResult::pass(),
        },
        Action::OpenModal(ModalKind::CommandPalette) => {
            state.modal.set_mode(InputMode::CommandPalette);
            // Command palette currently reuses the generic picker selection state.
            state.ui.table_picker.reset();
            DispatchResult::handled()
        }
        Action::CloseModal(ModalKind::SqlModal) => {
            state.modal.set_mode(InputMode::Normal);
            state.sql_modal.cleanup_on_close();
            state.flash_timers.clear(FlashId::SqlModal);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
