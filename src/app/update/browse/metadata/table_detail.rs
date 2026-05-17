use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::{Action, TableTarget};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_table_detail(state: &mut AppState, action: &Action) -> DispatchResult {
    match action {
        Action::TableDetailLoaded {
            dsn,
            run_id,
            detail,
            generation,
        } => {
            if state.session.dsn.as_ref() != Some(dsn)
                || !state.session.is_current_table_detail_run(*run_id)
            {
                return DispatchResult::handled();
            }

            if state.session.set_table_detail(*detail.clone(), *generation) {
                state.ui.inspector_scroll_offset = 0;
            }
            DispatchResult::handled()
        }
        Action::TableDetailFailed {
            dsn,
            run_id,
            error,
            generation,
        } => {
            if state.session.dsn.as_ref() != Some(dsn)
                || !state.session.is_current_table_detail_run(*run_id)
            {
                return DispatchResult::handled();
            }

            if *generation == state.session.selection_generation() {
                state.set_error(error.user_message());
            }
            DispatchResult::handled()
        }
        Action::LoadTableDetail(TableTarget {
            schema,
            table,
            generation,
        }) => {
            if let Some(dsn) = state.session.dsn.clone() {
                let run_id = state.session.begin_table_detail_run();
                DispatchResult::handled_with(vec![Effect::FetchTableDetail {
                    dsn,
                    schema: schema.clone(),
                    table: table.clone(),
                    generation: *generation,
                    run_id,
                }])
            } else {
                DispatchResult::handled()
            }
        }
        _ => DispatchResult::pass(),
    }
}
