use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::confirm_dialog::ConfirmIntent;
use crate::model::shared::input_mode::InputMode;
use crate::ports::outbound::AccessMode;
use crate::update::action::{Action, ScrollAmount, ScrollTarget};
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::reject_pending_mysql_connection_probe;

pub(super) fn reduce_confirm_dialog(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::Scroll {
            target: ScrollTarget::ConfirmDialog,
            direction,
            amount: ScrollAmount::Line,
        } => {
            state.confirm_dialog.scroll_preview(*direction);
            DispatchResult::handled()
        }
        Action::ConfirmDialogConfirm => {
            let intent = state.confirm_dialog.take_intent();
            state.modal.pop_mode();

            match intent {
                Some(ConfirmIntent::QuitNoConnection) => {
                    state.should_quit = true;
                    DispatchResult::handled_with(vec![Effect::CancelTrackedTasks])
                }
                Some(ConfirmIntent::DeleteConnection(id)) => {
                    DispatchResult::handled_with(vec![Effect::DeleteConnection { id }])
                }
                Some(ConfirmIntent::ExecuteWrite { blocked: true, .. }) => {
                    state.result_interaction.clear_write_preview();
                    state.query.clear_delete_refresh_target();
                    DispatchResult::handled()
                }
                Some(ConfirmIntent::ExecuteWrite {
                    sql,
                    blocked: false,
                }) => {
                    if reject_pending_mysql_connection_probe(state) {
                        state.result_interaction.clear_write_preview();
                        state.query.clear_delete_refresh_target();
                        return DispatchResult::handled();
                    }
                    if let Some(dsn) = state.session.dsn().map(String::from) {
                        let run_id = state.query.begin_non_preview_running(now);
                        DispatchResult::handled_with(vec![Effect::ExecuteWrite {
                            dsn,
                            run_id,
                            query: sql,
                            access_mode: AccessMode::from_read_only(state.session.is_read_only()),
                        }])
                    } else {
                        state.result_interaction.clear_write_preview();
                        state.query.clear_delete_refresh_target();
                        state.messages.set_error("No active connection".to_string());
                        DispatchResult::handled()
                    }
                }
                Some(ConfirmIntent::DisableReadOnly) => {
                    state.session.disable_read_only();
                    DispatchResult::handled()
                }
                Some(ConfirmIntent::CsvExportRerunnable {
                    dsn,
                    run_id,
                    export_query,
                    file_name,
                }) => {
                    if reject_pending_mysql_connection_probe(state) {
                        return DispatchResult::handled();
                    }
                    if state.is_stale_query_run(&dsn, run_id) {
                        DispatchResult::handled()
                    } else {
                        DispatchResult::handled_with(vec![Effect::ExportCsv {
                            dsn,
                            run_id,
                            query: export_query,
                            file_name,
                        }])
                    }
                }
                Some(ConfirmIntent::CsvExportCached {
                    dsn,
                    run_id,
                    file_name,
                    row_count,
                    snapshot,
                }) => {
                    if reject_pending_mysql_connection_probe(state) {
                        return DispatchResult::handled();
                    }
                    if state.is_stale_query_run(&dsn, run_id) {
                        DispatchResult::handled()
                    } else {
                        DispatchResult::handled_with(vec![Effect::ExportCsvFromCache {
                            dsn,
                            run_id,
                            file_name,
                            columns: snapshot.columns,
                            values: snapshot.values,
                            row_count,
                        }])
                    }
                }
                None => DispatchResult::handled(),
            }
        }
        Action::ConfirmDialogCancel => {
            let intent = state.confirm_dialog.take_intent();
            let canceling_current_csv_export = match &intent {
                Some(
                    ConfirmIntent::CsvExportRerunnable { dsn, run_id, .. }
                    | ConfirmIntent::CsvExportCached { dsn, run_id, .. },
                ) => !state.is_stale_query_run(dsn, *run_id),
                _ => false,
            };
            state.result_interaction.clear_write_preview();
            state.query.clear_delete_refresh_target();
            if canceling_current_csv_export {
                state.query.mark_idle();
            }

            if matches!(intent, Some(ConfirmIntent::QuitNoConnection)) {
                state.connection_setup.reset();
                if !state.connections().is_empty() || state.session.dsn().is_some() {
                    state.connection_setup.set_first_run(false);
                }
                state.modal.pop_mode_override(InputMode::ConnectionSetup);
                DispatchResult::handled()
            } else {
                state.modal.pop_mode();
                DispatchResult::handled()
            }
        }
        _ => DispatchResult::pass(),
    }
}
