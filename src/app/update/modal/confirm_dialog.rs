use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::confirm_dialog::ConfirmIntent;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, ScrollAmount, ScrollTarget};
use crate::update::dispatch_result::DispatchResult;

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
            let max_scroll = state.confirm_dialog.max_scroll() as usize;
            state.confirm_dialog.preview_scroll = direction.clamp_vertical_offset(
                state.confirm_dialog.preview_scroll as usize,
                max_scroll,
                1,
            ) as u16;
            DispatchResult::handled()
        }
        Action::ConfirmDialogConfirm => {
            let intent = state.confirm_dialog.take_intent();
            state.modal.pop_mode();

            match intent {
                Some(ConfirmIntent::QuitNoConnection) => {
                    state.should_quit = true;
                    DispatchResult::handled()
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
                    if let Some(dsn) = &state.session.dsn {
                        state.query.begin_running(now);
                        DispatchResult::handled_with(vec![Effect::ExecuteWrite {
                            dsn: dsn.clone(),
                            query: sql,
                            read_only: state.session.read_only,
                        }])
                    } else {
                        state.result_interaction.clear_write_preview();
                        state.query.clear_delete_refresh_target();
                        state
                            .messages
                            .set_error_at("No active connection".to_string(), now);
                        DispatchResult::handled()
                    }
                }
                Some(ConfirmIntent::DisableReadOnly) => {
                    state.session.read_only = false;
                    DispatchResult::handled()
                }
                Some(ConfirmIntent::CsvExport {
                    export_query,
                    file_name,
                    row_count,
                }) => {
                    if let Some(dsn) = &state.session.dsn {
                        DispatchResult::handled_with(vec![Effect::ExportCsv {
                            dsn: dsn.clone(),
                            query: export_query,
                            file_name,
                            row_count,
                            read_only: state.session.read_only,
                        }])
                    } else {
                        DispatchResult::handled()
                    }
                }
                None => DispatchResult::handled(),
            }
        }
        Action::ConfirmDialogCancel => {
            let intent = state.confirm_dialog.take_intent();
            state.result_interaction.clear_write_preview();
            state.query.clear_delete_refresh_target();

            if matches!(intent, Some(ConfirmIntent::QuitNoConnection)) {
                state.connection_setup.reset();
                if !state.connections().is_empty() || state.session.dsn.is_some() {
                    state.connection_setup.is_first_run = false;
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
