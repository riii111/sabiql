use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::{Action, TableTarget};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_table_detail(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::TableDetailLoaded {
            dsn,
            run_id,
            detail,
            generation,
        } => {
            if !state.session.dsn_matches(dsn)
                || !state.session.is_current_table_detail_run(*run_id)
            {
                return DispatchResult::handled();
            }

            if state.session.set_table_detail(*detail.clone(), *generation) {
                state.ui.set_inspector_scroll_offset(0);
                return DispatchResult::handled_with(reveal_pending_preview(state, *generation));
            }
            DispatchResult::handled()
        }
        Action::TableDetailFailed {
            dsn,
            run_id,
            error,
            generation,
        } => {
            if !state.session.dsn_matches(dsn)
                || !state.session.is_current_table_detail_run(*run_id)
            {
                return DispatchResult::handled();
            }

            let message = error.user_message();
            if state
                .session
                .mark_table_detail_failed(*generation, message.clone())
            {
                state.messages.set_error_at(message, now);
                return DispatchResult::handled_with(reveal_pending_preview(state, *generation));
            }
            DispatchResult::handled()
        }
        Action::LoadTableDetail(TableTarget {
            schema,
            table,
            generation,
        }) => {
            if !state
                .session
                .is_current_table_selection(schema, table, *generation)
            {
                return DispatchResult::handled();
            }

            let Some(dsn) = state.session.dsn().map(String::from) else {
                let message = "No active connection".to_string();
                if state
                    .session
                    .mark_table_detail_failed(*generation, message.clone())
                {
                    state.messages.set_error_at(message, now);
                }
                return DispatchResult::handled();
            };

            let run_id = state.session.begin_table_detail_run();
            DispatchResult::handled_with(vec![Effect::FetchTableDetail {
                dsn,
                schema: schema.clone(),
                table: table.clone(),
                generation: *generation,
                run_id,
            }])
        }
        _ => DispatchResult::pass(),
    }
}

fn reveal_pending_preview(state: &AppState, generation: u64) -> Vec<Effect> {
    if state.query.has_pending_preview(generation) {
        vec![Effect::DispatchActions(vec![
            Action::RevealPendingPreview { generation },
        ])]
    } else {
        vec![]
    }
}
