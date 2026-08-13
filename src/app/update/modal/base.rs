use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::DatabaseType;
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
            state.ui.set_database_picker(false);
            state.ui.table_picker_mut().clear_filter_and_reset();
            DispatchResult::handled()
        }
        Action::OpenModal(ModalKind::DatabasePicker) => {
            let Some(connection_id) = state.session.active_connection_id().cloned() else {
                return DispatchResult::handled();
            };
            if state.session.active_database_type() != Some(DatabaseType::MySQL) {
                return DispatchResult::handled();
            }
            let Some(dsn) = state.session.server_dsn() else {
                return DispatchResult::handled();
            };
            state.ui.set_database_picker(true);
            state.ui.table_picker_mut().clear_filter_and_reset();
            state.modal.set_mode(InputMode::TablePicker);
            DispatchResult::handled_with(vec![Effect::FetchMySqlDatabases {
                connection_id,
                dsn,
                connection_generation: state.session.connection_generation(),
                database_generation: state.session.database_generation(),
            }])
        }
        Action::CloseModal(
            ModalKind::TablePicker | ModalKind::DatabasePicker | ModalKind::CommandPalette,
        ) => {
            state.ui.set_database_picker(false);
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
            state.ui.table_picker_mut().reset();
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
