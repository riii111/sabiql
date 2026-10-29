use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::er_state::{ErDiagram, ErViewerCommand};
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, ErDiagramInfo, ModalKind};
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::reject_pending_mysql_connection_probe;
use crate::update::input::keybindings::global;

pub(super) fn reduce_diagram_lifecycle(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::ErDiagramOpened(ErDiagramInfo {
            run_id,
            tables,
            layout,
            total_tables,
        }) => {
            if !state.er_preparation.is_current_run(*run_id) {
                return DispatchResult::handled();
            }
            state.er_preparation.mark_idle();
            // Reset so next ErOpenDiagram re-evaluates target_tables from scratch.
            state.table_prefetch.invalidate_prefetch();
            // Never take over the keyboard from a view the user moved on to while
            // the diagram was being prepared.
            if state.input_mode() != InputMode::Normal {
                state.messages.set_success_at(
                    format!(
                        "ER diagram finished while another view was open — press {} to open it again",
                        global::ER_DIAGRAM.key_short
                    ),
                    now,
                );
                return DispatchResult::handled();
            }
            state.messages.set_success_at(
                format!(
                    "✓ ER diagram ready ({}/{total_tables} tables)",
                    tables.len()
                ),
                now,
            );
            state.er_diagram = Some(ErDiagram {
                id: *run_id,
                tables: tables.clone(),
                total_tables: *total_tables,
                layout: layout.clone(),
                has_selection: false,
            });
            state.modal.push_mode(InputMode::ErDiagram);
            DispatchResult::handled()
        }
        Action::ErViewerEscape => {
            if state.er_diagram.as_ref().is_some_and(|d| d.has_selection) {
                return DispatchResult::handled_with(vec![Effect::ApplyErViewerCommand(
                    ErViewerCommand::ClearSelection,
                )]);
            }
            reduce_diagram_lifecycle(state, &Action::CloseModal(ModalKind::ErDiagram), now)
        }
        Action::CloseModal(ModalKind::ErDiagram) => {
            state.er_diagram = None;
            if state.input_mode() == InputMode::ErDiagram {
                state.modal.pop_mode();
            }
            DispatchResult::handled()
        }
        Action::ErViewer(command) => {
            if state.er_diagram.is_none() {
                return DispatchResult::handled();
            }
            DispatchResult::handled_with(vec![Effect::ApplyErViewerCommand(*command)])
        }
        Action::ErDiagramFailed { run_id, error } => {
            if !state.er_preparation.is_current_run(*run_id) {
                return DispatchResult::handled();
            }
            state.er_preparation.mark_idle();
            state.messages.set_error(error.clone());
            DispatchResult::handled()
        }
        Action::ErLogWriteFailed(error) => {
            state.messages.set_error(error.clone());
            DispatchResult::handled()
        }
        Action::ErOpenDiagram => {
            if reject_pending_mysql_connection_probe(state) {
                return DispatchResult::handled();
            }
            if state.er_preparation.is_busy() {
                return DispatchResult::handled();
            }

            let Some(dsn) = state.session.dsn().map(String::from) else {
                state.messages.set_error("No active connection".to_string());
                return DispatchResult::handled();
            };
            if state.session.metadata().is_none() {
                state
                    .messages
                    .set_error("Metadata not loaded yet".to_string());
                return DispatchResult::handled();
            }

            state.table_prefetch.invalidate_prefetch();
            let run_id = state.er_preparation.start_waiting_run();
            state
                .messages
                .set_success_at("Checking for schema changes...".to_string(), now);

            DispatchResult::handled_with(vec![Effect::SmartErRefresh { dsn, run_id }])
        }
        Action::ErGenerateFromCache => {
            if !state.er_preparation.can_generate_from_cache() {
                return DispatchResult::handled();
            }

            state.er_preparation.mark_rendering();
            let run_id = state.er_preparation.run_id();
            let total_tables = state
                .session
                .metadata()
                .map_or(0, |m| m.table_summaries.len());

            DispatchResult::handled_with(vec![Effect::GenerateErDiagramFromCache {
                run_id,
                total_tables,
                target_tables: state.er_preparation.target_tables().to_vec(),
            }])
        }
        _ => DispatchResult::pass(),
    }
}
