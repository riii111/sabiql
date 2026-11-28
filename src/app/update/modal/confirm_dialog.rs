use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::confirm_dialog::ConfirmIntent;
use crate::model::shared::input_mode::InputMode;
use crate::ports::outbound::AccessMode;
use crate::update::action::{Action, ScrollAmount, ScrollDirection};
use crate::update::helpers::reject_pending_mysql_connection_probe;

pub(in crate::update) fn scroll_confirm_dialog(
    state: &mut AppState,
    direction: ScrollDirection,
    amount: ScrollAmount,
) {
    match amount {
        ScrollAmount::Line => {
            state.confirm_dialog.scroll_preview(direction);
        }
        _ => debug_assert!(
            false,
            "unsupported ConfirmDialog scroll: {direction:?} {amount:?}"
        ),
    }
}

pub(in crate::update) fn confirm_dialog_confirm(state: &mut AppState, now: Instant) -> Vec<Effect> {
    let intent = state.confirm_dialog.take_intent();
    state.modal.pop_mode();

    match intent {
        Some(ConfirmIntent::QuitNoConnection) => {
            state.should_quit = true;
            vec![Effect::CancelTrackedTasks]
        }
        Some(ConfirmIntent::DeleteConnection(id)) => {
            vec![Effect::DispatchActions(vec![Action::DeleteConnection(id)])]
        }
        Some(ConfirmIntent::ExecuteWrite { blocked: true, .. }) => {
            state.result_interaction.clear_write_preview();
            state.query.clear_delete_refresh_target();
            vec![]
        }
        Some(ConfirmIntent::ExecuteWrite {
            sql,
            blocked: false,
        }) => {
            if reject_pending_mysql_connection_probe(state) {
                state.result_interaction.clear_write_preview();
                state.query.clear_delete_refresh_target();
                return vec![];
            }
            if let Some(endpoint) = state.session.endpoint().cloned() {
                let run_id = state.query.begin_non_preview_running(now);
                vec![Effect::ExecuteWrite {
                    endpoint,
                    run_id,
                    query: sql,
                    access_mode: AccessMode::from_read_only(state.session.is_read_only()),
                }]
            } else {
                state.result_interaction.clear_write_preview();
                state.query.clear_delete_refresh_target();
                state.messages.set_error("No active connection".to_string());
                vec![]
            }
        }
        Some(ConfirmIntent::DisableReadOnly) => {
            state.session.disable_read_only();
            vec![]
        }
        Some(ConfirmIntent::CsvExportCached {
            endpoint,
            run_id,
            file_name,
            result,
        }) => {
            if reject_pending_mysql_connection_probe(state) {
                return vec![];
            }
            if state.is_stale_query_run(&endpoint, run_id) {
                vec![]
            } else {
                vec![Effect::ExportCsvFromCache {
                    run_id,
                    file_name,
                    result,
                }]
            }
        }
        None => vec![],
    }
}

pub(in crate::update) fn confirm_dialog_cancel(state: &mut AppState) -> Vec<Effect> {
    let intent = state.confirm_dialog.take_intent();
    let canceling_current_csv_export = match &intent {
        Some(ConfirmIntent::CsvExportCached {
            endpoint, run_id, ..
        }) => !state.is_stale_query_run(endpoint, *run_id),
        _ => false,
    };
    state.result_interaction.clear_write_preview();
    state.query.clear_delete_refresh_target();
    if canceling_current_csv_export {
        state.query.mark_idle();
    }

    if matches!(intent, Some(ConfirmIntent::QuitNoConnection)) {
        state.connection_setup.reset();
        if !state.connections().is_empty() || state.session.endpoint().is_some() {
            state.connection_setup.set_first_run(false);
        }
        state.modal.pop_mode_override(InputMode::ConnectionSetup);
        vec![]
    } else {
        state.modal.pop_mode();
        vec![]
    }
}
