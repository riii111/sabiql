use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::sql_editor::modal::sql_modal_visible_rows;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_completion(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        // Completion navigation
        Action::CompletionNext => {
            state.sql_modal.completion_next();
            DispatchResult::handled()
        }
        Action::CompletionPrev => {
            state.sql_modal.completion_prev();
            DispatchResult::handled()
        }
        Action::CompletionDismiss => {
            state.sql_modal.dismiss_completion();
            DispatchResult::handled()
        }
        // Completion accept
        Action::CompletionAccept => {
            state
                .sql_modal
                .accept_selected_completion(sql_modal_visible_rows(state.ui.terminal_height));
            DispatchResult::handled()
        }

        // Completion trigger/update
        Action::CompletionTrigger => DispatchResult::handled_with(vec![Effect::TriggerCompletion]),
        Action::CompletionUpdated {
            candidates,
            trigger_position,
            visible,
        } => {
            state
                .sql_modal
                .apply_completion_update(candidates, *trigger_position, *visible);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
