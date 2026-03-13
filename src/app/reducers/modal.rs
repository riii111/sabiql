//! Modal sub-reducer: modal/overlay toggles and confirm dialog.

use std::time::Instant;

use crate::app::action::Action;
use crate::app::confirm_dialog_state::ConfirmIntent;
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::query_execution::QueryStatus;
use crate::app::reducers::char_count;
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
        // Query History Picker
        Action::OpenQueryHistoryPicker => {
            // Guard: no-op when not connected, running, confirm dialog, or completion visible
            if state.runtime.active_connection_id.is_none() {
                return Some(vec![]);
            }
            if matches!(state.query.status, QueryStatus::Running) {
                return Some(vec![]);
            }
            if state.ui.input_mode == InputMode::ConfirmDialog {
                return Some(vec![]);
            }
            if state.sql_modal.completion.visible
                && !state.sql_modal.completion.candidates.is_empty()
            {
                return Some(vec![]);
            }

            let origin = state.ui.input_mode;
            state.query_history_picker.reset();
            state.query_history_picker.origin_mode = Some(origin);
            state.ui.input_mode = InputMode::QueryHistoryPicker;

            if let Some(conn_id) = &state.runtime.active_connection_id {
                Some(vec![Effect::LoadQueryHistory {
                    project_name: state.runtime.project_name.clone(),
                    connection_id: conn_id.clone(),
                }])
            } else {
                Some(vec![])
            }
        }
        Action::CloseQueryHistoryPicker => {
            let origin = state
                .query_history_picker
                .origin_mode
                .unwrap_or(InputMode::Normal);
            state.ui.input_mode = origin;
            state.query_history_picker.reset();
            Some(vec![])
        }
        Action::QueryHistoryLoaded(conn_id, entries) => {
            // Only apply if the picker is still open for this connection
            if state.ui.input_mode != InputMode::QueryHistoryPicker {
                return Some(vec![]);
            }
            if state.runtime.active_connection_id.as_ref() != Some(conn_id) {
                return Some(vec![]);
            }
            state.query_history_picker.entries = entries.clone();
            state.query_history_picker.selected = 0;
            state.query_history_picker.scroll_offset = 0;
            Some(vec![])
        }
        Action::QueryHistoryLoadFailed(msg) => {
            state.messages.last_error = Some(msg.clone());
            Some(vec![])
        }
        Action::QueryHistoryFilterInput(c) => {
            state.query_history_picker.filter_input.insert_char(*c);
            state.query_history_picker.selected = 0;
            state.query_history_picker.scroll_offset = 0;
            Some(vec![])
        }
        Action::QueryHistoryFilterBackspace => {
            state.query_history_picker.filter_input.backspace();
            state.query_history_picker.selected = 0;
            state.query_history_picker.scroll_offset = 0;
            Some(vec![])
        }
        Action::QueryHistorySelectNext => {
            let count = state.query_history_picker.filtered_count();
            if count > 0 && state.query_history_picker.selected < count - 1 {
                state.query_history_picker.selected += 1;
            }
            Some(vec![])
        }
        Action::QueryHistorySelectPrevious => {
            state.query_history_picker.selected =
                state.query_history_picker.selected.saturating_sub(1);
            Some(vec![])
        }
        Action::QueryHistoryConfirmSelection => {
            let filtered = state.query_history_picker.filtered_entries();
            let selected = state.query_history_picker.clamped_selected();
            let query = filtered.get(selected).map(|f| f.entry.query.clone());
            let origin = state
                .query_history_picker
                .origin_mode
                .unwrap_or(InputMode::Normal);

            state.query_history_picker.reset();

            let Some(query) = query else {
                // No entry selected (empty list or no match)
                state.ui.input_mode = origin;
                return Some(vec![]);
            };

            match origin {
                InputMode::Normal => {
                    // Open SqlModal and set content
                    state.ui.input_mode = InputMode::SqlModal;
                    state.sql_modal.status = crate::app::sql_modal_context::SqlModalStatus::Editing;
                    state.sql_modal.content = query;
                    state.sql_modal.cursor = char_count(&state.sql_modal.content);
                    state.sql_modal.completion.visible = false;
                    state.sql_modal.completion.candidates.clear();
                    state.sql_modal.completion.selected_index = 0;
                }
                InputMode::SqlModal => {
                    // Overwrite existing editor content
                    state.ui.input_mode = InputMode::SqlModal;
                    state.sql_modal.content = query;
                    state.sql_modal.cursor = char_count(&state.sql_modal.content);
                    state.sql_modal.status = crate::app::sql_modal_context::SqlModalStatus::Editing;
                }
                _ => {
                    state.ui.input_mode = origin;
                }
            }
            Some(vec![])
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
                            read_only: state.runtime.read_only,
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
                Some(ConfirmIntent::DisableReadOnly) => {
                    state.runtime.read_only = false;
                    Some(vec![])
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
                            read_only: state.runtime.read_only,
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
        fn disable_read_only_confirm_sets_read_only_false() {
            let mut state = create_test_state();
            state.runtime.read_only = true;
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.confirm_dialog.intent = Some(ConfirmIntent::DisableReadOnly);
            state.confirm_dialog.return_mode = InputMode::Normal;

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert!(!state.runtime.read_only);
            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert!(effects.is_empty());
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

    mod query_history_picker {
        use super::*;
        use crate::domain::ConnectionId;
        use crate::domain::query_history::QueryHistoryEntry;

        fn connected_state() -> AppState {
            let mut state = create_test_state();
            state.runtime.active_connection_id = Some(ConnectionId::from_string("test-conn"));
            state.runtime.project_name = "test-project".to_string();
            state.ui.input_mode = InputMode::Normal;
            state
        }

        #[test]
        fn open_when_not_connected_is_noop() {
            let mut state = create_test_state();
            state.runtime.active_connection_id = None;

            let effects =
                reduce_modal(&mut state, &Action::OpenQueryHistoryPicker, Instant::now()).unwrap();

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert!(effects.is_empty());
        }

        #[test]
        fn open_when_running_is_noop() {
            let mut state = connected_state();
            state.query.status = QueryStatus::Running;

            let effects =
                reduce_modal(&mut state, &Action::OpenQueryHistoryPicker, Instant::now()).unwrap();

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert!(effects.is_empty());
        }

        #[test]
        fn open_from_normal_sets_mode_and_emits_load_effect() {
            let mut state = connected_state();

            let effects =
                reduce_modal(&mut state, &Action::OpenQueryHistoryPicker, Instant::now()).unwrap();

            assert_eq!(state.ui.input_mode, InputMode::QueryHistoryPicker);
            assert_eq!(
                state.query_history_picker.origin_mode,
                Some(InputMode::Normal)
            );
            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::LoadQueryHistory { .. }));
        }

        #[test]
        fn close_restores_origin_mode() {
            let mut state = connected_state();
            state.ui.input_mode = InputMode::QueryHistoryPicker;
            state.query_history_picker.origin_mode = Some(InputMode::SqlModal);

            let effects =
                reduce_modal(&mut state, &Action::CloseQueryHistoryPicker, Instant::now()).unwrap();

            assert_eq!(state.ui.input_mode, InputMode::SqlModal);
            assert!(effects.is_empty());
        }

        #[test]
        fn loaded_stores_entries() {
            let mut state = connected_state();
            state.ui.input_mode = InputMode::QueryHistoryPicker;
            let conn_id = ConnectionId::from_string("test-conn");
            let entries = vec![QueryHistoryEntry::new(
                "SELECT 1".to_string(),
                "2026-03-13T12:00:00Z".to_string(),
                conn_id.clone(),
            )];

            let effects = reduce_modal(
                &mut state,
                &Action::QueryHistoryLoaded(conn_id, entries.clone()),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.query_history_picker.entries.len(), 1);
            assert!(effects.is_empty());
        }

        #[test]
        fn loaded_ignores_stale_connection() {
            let mut state = connected_state();
            state.ui.input_mode = InputMode::QueryHistoryPicker;
            let stale_conn = ConnectionId::from_string("old-conn");
            let entries = vec![QueryHistoryEntry::new(
                "SELECT 1".to_string(),
                "2026-03-13T12:00:00Z".to_string(),
                stale_conn.clone(),
            )];

            reduce_modal(
                &mut state,
                &Action::QueryHistoryLoaded(stale_conn, entries),
                Instant::now(),
            )
            .unwrap();

            assert!(state.query_history_picker.entries.is_empty());
        }

        #[test]
        fn loaded_ignores_when_picker_closed() {
            let mut state = connected_state();
            state.ui.input_mode = InputMode::Normal;
            let conn_id = ConnectionId::from_string("test-conn");
            let entries = vec![QueryHistoryEntry::new(
                "SELECT 1".to_string(),
                "2026-03-13T12:00:00Z".to_string(),
                conn_id.clone(),
            )];

            reduce_modal(
                &mut state,
                &Action::QueryHistoryLoaded(conn_id, entries),
                Instant::now(),
            )
            .unwrap();

            assert!(state.query_history_picker.entries.is_empty());
        }

        #[test]
        fn load_failed_sets_error_message() {
            let mut state = connected_state();

            reduce_modal(
                &mut state,
                &Action::QueryHistoryLoadFailed("disk error".to_string()),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.messages.last_error.as_deref(), Some("disk error"));
        }

        #[test]
        fn filter_input_resets_selection() {
            let mut state = connected_state();
            state.query_history_picker.selected = 5;

            let effects = reduce_modal(
                &mut state,
                &Action::QueryHistoryFilterInput('a'),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.query_history_picker.selected, 0);
            assert_eq!(state.query_history_picker.filter_input.content(), "a");
            assert!(effects.is_empty());
        }

        #[test]
        fn confirm_sets_cursor_to_char_count_not_byte_len() {
            let mut state = connected_state();
            state.ui.input_mode = InputMode::QueryHistoryPicker;
            state.query_history_picker.origin_mode = Some(InputMode::Normal);
            // 「SELECT 'あいう'」: 13 chars but 19 bytes
            let query = "SELECT '\u{3042}\u{3044}\u{3046}'".to_string();
            let expected_chars = query.chars().count(); // 13
            assert_ne!(query.len(), expected_chars); // sanity: bytes != chars
            state.query_history_picker.entries = vec![QueryHistoryEntry::new(
                query,
                "2026-03-13T12:00:00Z".to_string(),
                ConnectionId::from_string("test-conn"),
            )];

            reduce_modal(
                &mut state,
                &Action::QueryHistoryConfirmSelection,
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.sql_modal.cursor, expected_chars);
        }

        #[test]
        fn confirm_from_normal_opens_sql_modal_with_query() {
            let mut state = connected_state();
            state.ui.input_mode = InputMode::QueryHistoryPicker;
            state.query_history_picker.origin_mode = Some(InputMode::Normal);
            state.query_history_picker.entries = vec![QueryHistoryEntry::new(
                "SELECT * FROM users".to_string(),
                "2026-03-13T12:00:00Z".to_string(),
                ConnectionId::from_string("test-conn"),
            )];
            state.query_history_picker.selected = 0;

            let effects = reduce_modal(
                &mut state,
                &Action::QueryHistoryConfirmSelection,
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.ui.input_mode, InputMode::SqlModal);
            assert_eq!(state.sql_modal.content, "SELECT * FROM users");
            assert!(effects.is_empty());
        }

        #[test]
        fn confirm_from_sql_modal_overwrites_editor_content() {
            let mut state = connected_state();
            state.ui.input_mode = InputMode::QueryHistoryPicker;
            state.query_history_picker.origin_mode = Some(InputMode::SqlModal);
            state.sql_modal.content = "old query".to_string();
            state.query_history_picker.entries = vec![QueryHistoryEntry::new(
                "new query".to_string(),
                "2026-03-13T12:00:00Z".to_string(),
                ConnectionId::from_string("test-conn"),
            )];
            state.query_history_picker.selected = 0;

            reduce_modal(
                &mut state,
                &Action::QueryHistoryConfirmSelection,
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.ui.input_mode, InputMode::SqlModal);
            assert_eq!(state.sql_modal.content, "new query");
        }

        #[test]
        fn confirm_with_empty_entries_is_noop() {
            let mut state = connected_state();
            state.ui.input_mode = InputMode::QueryHistoryPicker;
            state.query_history_picker.origin_mode = Some(InputMode::Normal);

            reduce_modal(
                &mut state,
                &Action::QueryHistoryConfirmSelection,
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.ui.input_mode, InputMode::Normal);
        }

        #[test]
        fn select_next_increments() {
            let mut state = connected_state();
            state.query_history_picker.entries = vec![
                QueryHistoryEntry::new(
                    "SELECT 1".to_string(),
                    "2026-03-13T12:00:00Z".to_string(),
                    ConnectionId::from_string("test-conn"),
                ),
                QueryHistoryEntry::new(
                    "SELECT 2".to_string(),
                    "2026-03-13T12:00:00Z".to_string(),
                    ConnectionId::from_string("test-conn"),
                ),
            ];
            state.query_history_picker.selected = 0;

            reduce_modal(&mut state, &Action::QueryHistorySelectNext, Instant::now()).unwrap();

            assert_eq!(state.query_history_picker.selected, 1);
        }

        #[test]
        fn select_next_clamps_at_end() {
            let mut state = connected_state();
            state.query_history_picker.entries = vec![QueryHistoryEntry::new(
                "SELECT 1".to_string(),
                "2026-03-13T12:00:00Z".to_string(),
                ConnectionId::from_string("test-conn"),
            )];
            state.query_history_picker.selected = 0;

            reduce_modal(&mut state, &Action::QueryHistorySelectNext, Instant::now()).unwrap();

            assert_eq!(state.query_history_picker.selected, 0);
        }

        #[test]
        fn select_previous_decrements() {
            let mut state = connected_state();
            state.query_history_picker.selected = 1;

            reduce_modal(
                &mut state,
                &Action::QueryHistorySelectPrevious,
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.query_history_picker.selected, 0);
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
