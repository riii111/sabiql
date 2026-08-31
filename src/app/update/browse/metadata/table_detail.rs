use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_table_detail(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        Action::TableDetailLoaded {
            dsn,
            run_id,
            outcome,
            generation,
        } => {
            if !state.session.dsn_matches(dsn)
                || !state.session.is_current_table_detail_run(*run_id)
            {
                return DispatchResult::handled();
            }

            let effects = match outcome {
                Ok(detail) if state.session.set_table_detail(*detail.clone(), *generation) => {
                    state.ui.set_inspector_scroll_offset(0);
                    reveal_pending_preview(state, *generation)
                }
                Ok(_) => Vec::new(),
                Err(error) => {
                    let message = error.user_message();
                    if state
                        .session
                        .mark_table_detail_failed(*generation, message.clone())
                    {
                        state.messages.set_error(message);
                        reveal_pending_preview(state, *generation)
                    } else {
                        Vec::new()
                    }
                }
            };
            DispatchResult::handled_with(effects)
        }
        _ => DispatchResult::pass(),
    }
}

pub(super) fn reveal_pending_preview(state: &AppState, generation: u64) -> Vec<Effect> {
    if state.query.has_pending_preview(generation) {
        vec![Effect::DispatchActions(vec![
            Action::RevealPendingPreview { generation },
        ])]
    } else {
        vec![]
    }
}
