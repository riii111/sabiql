use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::modal::sql_modal_visible_rows;
use crate::update::action::{Action, CursorMove, ModalKind};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_mode(state: &mut AppState, action: &Action, _now: Instant) -> DispatchResult {
    match action {
        // Modal open/submit
        Action::OpenModal(ModalKind::SqlModal) => {
            state.modal.set_mode(InputMode::SqlModal);
            state.sql_modal.open_sql_tab();
            state.flash_timers.clear(FlashId::SqlModal);
            if !state.sql_modal.is_prefetch_started() && state.session.metadata().is_some() {
                DispatchResult::handled_with(vec![Effect::DispatchActions(vec![
                    Action::StartPrefetchAll,
                ])])
            } else {
                DispatchResult::handled()
            }
        }
        Action::SqlModalAppendInsert => {
            state.sql_modal.editor.move_cursor(CursorMove::LineEnd);
            state
                .sql_modal
                .editor
                .update_scroll(sql_modal_visible_rows(state.ui.terminal_height));
            state.sql_modal.enter_editing();
            DispatchResult::handled()
        }
        Action::SqlModalEnterInsert => {
            state.sql_modal.enter_editing();
            DispatchResult::handled()
        }
        Action::SqlModalEnterNormal => {
            state.sql_modal.enter_normal();
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
