//! Modal sub-reducer: modal/overlay toggles and confirm dialog.

use std::time::Instant;

use crate::app::action::Action;
use crate::app::confirm_dialog_state::ConfirmIntent;
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::query_execution::QueryStatus;
use crate::app::state::AppState;

/// Handles modal/overlay toggles and confirm dialog actions.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_modal(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::OpenTablePicker => {
            state.ui.input_mode = InputMode::TablePicker;
            state.ui.filter_input.clear();
            state.ui.reset_picker_selection();
            Some(vec![])
        }
        Action::CloseTablePicker => {
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::OpenCommandPalette => {
            state.ui.input_mode = InputMode::CommandPalette;
            state.ui.reset_picker_selection();
            Some(vec![])
        }
        Action::CloseCommandPalette => {
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::OpenHelp => {
            state.ui.input_mode = if state.ui.input_mode == InputMode::Help {
                InputMode::Normal
            } else {
                InputMode::Help
            };
            Some(vec![])
        }
        Action::CloseHelp => {
            state.ui.input_mode = InputMode::Normal;
            state.ui.help_scroll_offset = 0;
            Some(vec![])
        }
        Action::HelpScrollUp => {
            state.ui.help_scroll_offset = state.ui.help_scroll_offset.saturating_sub(1);
            Some(vec![])
        }
        Action::HelpScrollDown => {
            let max_scroll = state.ui.help_max_scroll();
            if state.ui.help_scroll_offset < max_scroll {
                state.ui.help_scroll_offset += 1;
            }
            Some(vec![])
        }
        Action::CloseSqlModal => {
            state.ui.input_mode = InputMode::Normal;
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion_debounce = None;
            Some(vec![])
        }
        Action::OpenErTablePicker => {
            if state.cache.metadata.is_none() {
                state.ui.pending_er_picker = true;
                state.set_success("Waiting for metadata...".to_string());
                return Some(vec![]);
            }
            state.ui.pending_er_picker = false;
            state.ui.er_selected_tables.clear();
            state.ui.input_mode = InputMode::ErTablePicker;
            state.ui.er_filter_input.clear();
            state.ui.reset_er_picker_selection();
            Some(vec![])
        }
        Action::CloseErTablePicker => {
            state.ui.input_mode = InputMode::Normal;
            state.ui.er_filter_input.clear();
            state.ui.er_selected_tables.clear();
            state.ui.pending_er_picker = false;
            Some(vec![])
        }
        Action::ErFilterInput(c) => {
            state.ui.er_filter_input.push(*c);
            state.ui.reset_er_picker_selection();
            Some(vec![])
        }
        Action::ErFilterBackspace => {
            state.ui.er_filter_input.pop();
            state.ui.reset_er_picker_selection();
            Some(vec![])
        }
        Action::ErToggleSelection => {
            let filtered = state.er_filtered_tables();
            if let Some(table) = filtered.get(state.ui.er_picker_selected) {
                let name = table.qualified_name();
                if !state.ui.er_selected_tables.remove(&name) {
                    state.ui.er_selected_tables.insert(name);
                }
            }
            Some(vec![])
        }
        Action::ErSelectAll => {
            let all_tables: Vec<String> =
                state.tables().iter().map(|t| t.qualified_name()).collect();
            if state.ui.er_selected_tables.len() == all_tables.len() {
                state.ui.er_selected_tables.clear();
            } else {
                state.ui.er_selected_tables = all_tables.into_iter().collect();
            }
            Some(vec![])
        }
        Action::ErConfirmSelection => {
            if state.ui.er_selected_tables.is_empty() {
                state.set_error("No tables selected".to_string());
                return Some(vec![]);
            }
            state.er_preparation.target_tables =
                state.ui.er_selected_tables.iter().cloned().collect();
            state.ui.input_mode = InputMode::Normal;
            state.ui.er_filter_input.clear();
            state.ui.er_selected_tables.clear();
            Some(vec![Effect::DispatchActions(vec![Action::ErOpenDiagram])])
        }
        Action::Escape => {
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }

        // Confirm Dialog
        Action::ConfirmDialogConfirm => {
            let intent = state.confirm_dialog.intent.take();
            let return_mode =
                std::mem::replace(&mut state.confirm_dialog.return_mode, InputMode::Normal);
            state.ui.input_mode = return_mode;

            match intent {
                Some(ConfirmIntent::QuitNoConnection) => {
                    state.should_quit = true;
                    Some(vec![])
                }
                Some(ConfirmIntent::DeleteConnection(id)) => {
                    Some(vec![Effect::DeleteConnection { id }])
                }
                Some(ConfirmIntent::ExecuteWrite { blocked: true, .. }) => {
                    state.pending_write_preview = None;
                    state.query.pending_delete_refresh_target = None;
                    Some(vec![])
                }
                Some(ConfirmIntent::ExecuteWrite {
                    sql,
                    blocked: false,
                }) => {
                    if let Some(dsn) = &state.runtime.dsn {
                        state.query.status = QueryStatus::Running;
                        state.query.start_time = Some(now);
                        Some(vec![Effect::ExecuteWrite {
                            dsn: dsn.clone(),
                            query: sql,
                        }])
                    } else {
                        state.pending_write_preview = None;
                        state.query.pending_delete_refresh_target = None;
                        state
                            .messages
                            .set_error_at("No active connection".to_string(), now);
                        Some(vec![])
                    }
                }
                Some(ConfirmIntent::CsvExport {
                    export_query,
                    file_name,
                    row_count,
                }) => {
                    if let Some(dsn) = &state.runtime.dsn {
                        Some(vec![Effect::ExportCsv {
                            dsn: dsn.clone(),
                            query: export_query,
                            file_name,
                            row_count,
                        }])
                    } else {
                        Some(vec![])
                    }
                }
                None => Some(vec![]),
            }
        }
        Action::ConfirmDialogCancel => {
            let intent = state.confirm_dialog.intent.take();
            state.pending_write_preview = None;
            state.query.pending_delete_refresh_target = None;
            let return_mode =
                std::mem::replace(&mut state.confirm_dialog.return_mode, InputMode::Normal);

            match intent {
                Some(ConfirmIntent::QuitNoConnection) => {
                    // Restore ConnectionSetup synchronously to avoid 1-tick flicker
                    state.connection_setup.reset();
                    if !state.connections().is_empty() || state.runtime.dsn.is_some() {
                        state.connection_setup.is_first_run = false;
                    }
                    state.ui.input_mode = InputMode::ConnectionSetup;
                    Some(vec![])
                }
                _ => {
                    state.ui.input_mode = return_mode;
                    Some(vec![])
                }
            }
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::confirm_dialog_state::ConfirmIntent;

    use std::time::Instant;

    fn create_test_state() -> AppState {
        AppState::new("test".to_string())
    }

    mod confirm_dialog_confirm {
        use super::*;

        #[test]
        fn quit_no_connection_sets_should_quit() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.confirm_dialog.intent = Some(ConfirmIntent::QuitNoConnection);

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert!(state.should_quit);
            assert!(state.confirm_dialog.intent.is_none());
            assert!(effects.is_empty());
        }

        #[test]
        fn delete_connection_returns_delete_effect() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            let id = crate::domain::ConnectionId::new();
            state.confirm_dialog.intent = Some(ConfirmIntent::DeleteConnection(id.clone()));
            state.confirm_dialog.return_mode = InputMode::ConnectionSelector;

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert_eq!(state.ui.input_mode, InputMode::ConnectionSelector);
            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::DeleteConnection { .. }));
        }

        #[test]
        fn execute_write_sets_running_state_and_returns_effect() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.confirm_dialog.intent = Some(ConfirmIntent::ExecuteWrite {
                sql: "UPDATE t SET x=1".to_string(),
                blocked: false,
            });
            state.confirm_dialog.return_mode = InputMode::CellEdit;

            let now = Instant::now();
            let effects = reduce_modal(&mut state, &Action::ConfirmDialogConfirm, now).unwrap();

            assert_eq!(state.ui.input_mode, InputMode::CellEdit);
            assert!(matches!(state.query.status, QueryStatus::Running));
            assert!(state.query.start_time.is_some());
            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::ExecuteWrite { .. }));
        }

        #[test]
        fn execute_write_no_dsn_sets_error() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.runtime.dsn = None;
            state.confirm_dialog.intent = Some(ConfirmIntent::ExecuteWrite {
                sql: "UPDATE t SET x=1".to_string(),
                blocked: false,
            });

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("No active connection")
            );
        }

        #[test]
        fn execute_write_blocked_returns_to_mode_with_no_effects() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.confirm_dialog.intent = Some(ConfirmIntent::ExecuteWrite {
                sql: "UPDATE t SET x=1".to_string(),
                blocked: true,
            });
            state.confirm_dialog.return_mode = InputMode::Normal;

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert!(effects.is_empty());
        }

        #[test]
        fn execute_write_blocked_confirm_clears_preview_state() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.pending_write_preview = Some(crate::app::write_guardrails::WritePreview {
                operation: crate::app::write_guardrails::WriteOperation::Update,
                sql: "UPDATE t SET x=1".to_string(),
                target_summary: crate::app::write_guardrails::TargetSummary {
                    schema: "public".to_string(),
                    table: "t".to_string(),
                    key_values: vec![],
                },
                diff: vec![],
                guardrail: crate::app::write_guardrails::GuardrailDecision {
                    risk_level: crate::app::write_guardrails::RiskLevel::High,
                    blocked: true,
                    reason: Some("too risky".to_string()),
                    target_summary: None,
                },
            });
            state.query.pending_delete_refresh_target = Some((0, None, 1));
            state.confirm_dialog.intent = Some(ConfirmIntent::ExecuteWrite {
                sql: "UPDATE t SET x=1".to_string(),
                blocked: true,
            });

            reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert!(state.pending_write_preview.is_none());
            assert!(state.query.pending_delete_refresh_target.is_none());
        }

        #[test]
        fn csv_export_returns_export_effect() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.confirm_dialog.intent = Some(ConfirmIntent::CsvExport {
                export_query: "SELECT 1".to_string(),
                file_name: "test.csv".to_string(),
                row_count: Some(200_000),
            });

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::ExportCsv { .. }));
        }

        #[test]
        fn none_intent_confirm_does_not_panic() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.confirm_dialog.intent = None;

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }
    }

    mod confirm_dialog_cancel {
        use super::*;

        #[test]
        fn quit_no_connection_restores_connection_setup_synchronously() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.confirm_dialog.intent = Some(ConfirmIntent::QuitNoConnection);

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogCancel, Instant::now()).unwrap();

            assert_eq!(state.ui.input_mode, InputMode::ConnectionSetup);
            assert!(effects.is_empty());
        }

        #[test]
        fn other_intents_cancel_returns_empty_effects() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.confirm_dialog.intent = Some(ConfirmIntent::ExecuteWrite {
                sql: "UPDATE t SET x=1".to_string(),
                blocked: false,
            });
            state.confirm_dialog.return_mode = InputMode::CellEdit;

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogCancel, Instant::now()).unwrap();

            assert_eq!(state.ui.input_mode, InputMode::CellEdit);
            assert!(effects.is_empty());
            assert!(state.pending_write_preview.is_none());
        }

        #[test]
        fn none_intent_cancel_does_not_panic() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.confirm_dialog.intent = None;

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogCancel, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }
    }
}
