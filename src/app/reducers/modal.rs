use std::time::Instant;

use crate::app::action::{
    Action, InputTarget, ListMotion, ListTarget, ScrollAmount, ScrollDirection, ScrollTarget,
};
use crate::app::confirm_dialog_state::ConfirmIntent;
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::reducers::char_count;
use crate::app::sql_modal_context::{SqlModalStatus, SqlModalTab};
use crate::app::state::AppState;

pub fn reduce_modal(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::OpenTablePicker => {
            state.modal.set_mode(InputMode::TablePicker);
            state.ui.table_picker.filter_input.clear();
            state.ui.table_picker.reset();
            Some(vec![])
        }
        Action::CloseTablePicker => {
            state.modal.set_mode(InputMode::Normal);
            Some(vec![])
        }
        Action::OpenCommandPalette => {
            state.modal.set_mode(InputMode::CommandPalette);
            state.ui.table_picker.reset();
            Some(vec![])
        }
        Action::CloseCommandPalette => {
            state.modal.set_mode(InputMode::Normal);
            Some(vec![])
        }
        Action::OpenHelp => {
            if state.modal.active_mode() == InputMode::Help {
                state.modal.set_mode(InputMode::Normal);
            } else {
                state.modal.set_mode(InputMode::Help);
            }
            Some(vec![])
        }
        Action::CloseHelp => {
            state.modal.set_mode(InputMode::Normal);
            state.ui.help_scroll_offset = 0;
            Some(vec![])
        }
        Action::Scroll {
            target: ScrollTarget::Help,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        } => {
            state.ui.help_scroll_offset = state.ui.help_scroll_offset.saturating_sub(1);
            Some(vec![])
        }
        Action::Scroll {
            target: ScrollTarget::Help,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        } => {
            let max_scroll = state.ui.help_max_scroll();
            if state.ui.help_scroll_offset < max_scroll {
                state.ui.help_scroll_offset += 1;
            }
            Some(vec![])
        }
        Action::CloseSqlModal => {
            state.modal.set_mode(InputMode::Normal);
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion_debounce = None;
            state.sql_modal.yank_flash_until = None;
            Some(vec![])
        }
        Action::OpenErTablePicker => {
            if state.session.metadata().is_none() {
                state.ui.pending_er_picker = true;
                state.set_success("Waiting for metadata...".to_string());
                return Some(vec![]);
            }
            state.ui.pending_er_picker = false;
            state.ui.er_selected_tables.clear();
            state.modal.set_mode(InputMode::ErTablePicker);
            state.ui.er_picker.filter_input.clear();
            state.ui.er_picker.reset();
            Some(vec![])
        }
        Action::CloseErTablePicker => {
            state.modal.set_mode(InputMode::Normal);
            state.ui.er_picker.filter_input.clear();
            state.ui.er_selected_tables.clear();
            state.ui.pending_er_picker = false;
            Some(vec![])
        }
        Action::TextInput {
            target: InputTarget::ErFilter,
            ch: c,
        } => {
            state.ui.er_picker.filter_input.push(*c);
            state.ui.er_picker.reset();
            Some(vec![])
        }
        Action::TextBackspace {
            target: InputTarget::ErFilter,
        } => {
            state.ui.er_picker.filter_input.pop();
            state.ui.er_picker.reset();
            Some(vec![])
        }
        Action::ErToggleSelection => {
            let filtered = state.er_filtered_tables();
            if let Some(table) = filtered.get(state.ui.er_picker.selected()) {
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
            state.modal.set_mode(InputMode::Normal);
            state.ui.er_picker.filter_input.clear();
            state.ui.er_selected_tables.clear();
            Some(vec![Effect::DispatchActions(vec![Action::ErOpenDiagram])])
        }
        // Query History Picker
        Action::OpenQueryHistoryPicker => {
            if state.session.active_connection_id.is_none() {
                return Some(vec![]);
            }
            if state.query.is_running() {
                return Some(vec![]);
            }
            if state.modal.active_mode() == InputMode::ConfirmDialog {
                return Some(vec![]);
            }
            if state.sql_modal.completion.visible
                && !state.sql_modal.completion.candidates.is_empty()
            {
                return Some(vec![]);
            }

            state.query_history_picker.reset();
            state.modal.push_mode(InputMode::QueryHistoryPicker);

            let conn_id = state.session.active_connection_id.as_ref().unwrap();
            Some(vec![Effect::LoadQueryHistory {
                project_name: state.runtime.project_name.clone(),
                connection_id: conn_id.clone(),
            }])
        }
        Action::CloseQueryHistoryPicker => {
            state.modal.pop_mode();
            state.query_history_picker.reset();
            Some(vec![])
        }
        Action::QueryHistoryLoaded(conn_id, entries) => {
            if state.modal.active_mode() != InputMode::QueryHistoryPicker {
                return Some(vec![]);
            }
            if state.session.active_connection_id.as_ref() != Some(conn_id) {
                return Some(vec![]);
            }
            state.query_history_picker.entries = entries.clone();
            state.query_history_picker.selected = 0;
            state.query_history_picker.scroll_offset = 0;
            Some(vec![])
        }
        Action::QueryHistoryLoadFailed(msg) => {
            state.messages.set_error_at(msg.clone(), now);
            Some(vec![])
        }
        Action::TextInput {
            target: InputTarget::QueryHistoryFilter,
            ch: c,
        } => {
            state.query_history_picker.filter_input.insert_char(*c);
            state.query_history_picker.selected = 0;
            state.query_history_picker.scroll_offset = 0;
            Some(vec![])
        }
        Action::TextBackspace {
            target: InputTarget::QueryHistoryFilter,
        } => {
            state.query_history_picker.filter_input.backspace();
            state.query_history_picker.selected = 0;
            state.query_history_picker.scroll_offset = 0;
            Some(vec![])
        }
        Action::ListSelect {
            target: ListTarget::QueryHistory,
            motion: ListMotion::Next,
        } => {
            let count = state.query_history_picker.grouped_count();
            if count > 0 && state.query_history_picker.selected < count - 1 {
                state.query_history_picker.selected += 1;
            }
            Some(vec![])
        }
        Action::ListSelect {
            target: ListTarget::QueryHistory,
            motion: ListMotion::Previous,
        } => {
            state.query_history_picker.selected =
                state.query_history_picker.selected.saturating_sub(1);
            Some(vec![])
        }
        Action::QueryHistoryConfirmSelection => {
            let grouped = state.query_history_picker.grouped_filtered_entries();
            let selected = state.query_history_picker.clamped_selected();
            let query = grouped.get(selected).map(|g| g.entry.query.clone());
            let origin = state.modal.pop_mode();

            state.query_history_picker.reset();

            let Some(query) = query else {
                return Some(vec![]);
            };

            match origin {
                InputMode::Normal => {
                    state.modal.set_mode(InputMode::SqlModal);
                    state
                        .sql_modal
                        .set_status(crate::app::sql_modal_context::SqlModalStatus::Editing);
                    state.sql_modal.content = query;
                    state.sql_modal.cursor = char_count(&state.sql_modal.content);
                    state.sql_modal.completion.visible = false;
                    state.sql_modal.completion.candidates.clear();
                    state.sql_modal.completion.selected_index = 0;
                }
                InputMode::SqlModal => {
                    state.sql_modal.content = query;
                    state.sql_modal.cursor = char_count(&state.sql_modal.content);
                    state
                        .sql_modal
                        .set_status(crate::app::sql_modal_context::SqlModalStatus::Editing);
                }
                _ => {}
            }
            Some(vec![])
        }

        Action::Escape => {
            state.modal.set_mode(InputMode::Normal);
            Some(vec![])
        }

        // Confirm Dialog
        Action::ConfirmDialogConfirm => {
            let intent = state.confirm_dialog.take_intent();
            state.modal.pop_mode();

            match intent {
                Some(ConfirmIntent::QuitNoConnection) => {
                    state.should_quit = true;
                    Some(vec![])
                }
                Some(ConfirmIntent::DeleteConnection(id)) => {
                    Some(vec![Effect::DeleteConnection { id }])
                }
                Some(ConfirmIntent::ExecuteWrite { blocked: true, .. }) => {
                    state.result_interaction.clear_write_preview();
                    state.query.clear_delete_refresh_target();
                    Some(vec![])
                }
                Some(ConfirmIntent::ExecuteWrite {
                    sql,
                    blocked: false,
                }) => {
                    if let Some(dsn) = &state.session.dsn {
                        state.query.begin_running(now);
                        Some(vec![Effect::ExecuteWrite {
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
                        Some(vec![])
                    }
                }
                Some(ConfirmIntent::DisableReadOnly) => {
                    state.session.read_only = false;
                    Some(vec![])
                }
                Some(ConfirmIntent::CsvExport {
                    export_query,
                    file_name,
                    row_count,
                }) => {
                    if let Some(dsn) = &state.session.dsn {
                        Some(vec![Effect::ExportCsv {
                            dsn: dsn.clone(),
                            query: export_query,
                            file_name,
                            row_count,
                            read_only: state.session.read_only,
                        }])
                    } else {
                        Some(vec![])
                    }
                }
                Some(ConfirmIntent::ExplainAnalyze { query, .. }) => {
                    if let Some(dsn) = &state.session.dsn {
                        let explain_query = format!("EXPLAIN ANALYZE {}", query);
                        state.sql_modal.set_status(SqlModalStatus::Running);
                        state.sql_modal.active_tab = SqlModalTab::Plan;
                        state.explain.reset();
                        state.query.begin_running(now);
                        Some(vec![Effect::ExecuteExplain {
                            dsn: dsn.clone(),
                            query: explain_query,
                            is_analyze: true,
                            read_only: state.session.read_only,
                        }])
                    } else {
                        Some(vec![])
                    }
                }
                None => Some(vec![]),
            }
        }
        Action::ConfirmDialogCancel => {
            let intent = state.confirm_dialog.take_intent();
            state.result_interaction.clear_write_preview();
            state.query.clear_delete_refresh_target();

            match intent {
                Some(ConfirmIntent::QuitNoConnection) => {
                    state.connection_setup.reset();
                    if !state.connections().is_empty() || state.session.dsn.is_some() {
                        state.connection_setup.is_first_run = false;
                    }
                    state.modal.pop_mode_override(InputMode::ConnectionSetup);
                    Some(vec![])
                }
                _ => {
                    state.modal.pop_mode();
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

        pub(super) fn enter_confirm_dialog(state: &mut AppState, return_mode: InputMode) {
            state.modal.set_mode(return_mode);
            state.modal.push_mode(InputMode::ConfirmDialog);
        }

        #[test]
        fn quit_no_connection_sets_should_quit() {
            let mut state = create_test_state();
            enter_confirm_dialog(&mut state, InputMode::Normal);
            state
                .confirm_dialog
                .open("", "", ConfirmIntent::QuitNoConnection);

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert!(state.should_quit);
            assert!(state.confirm_dialog.intent().is_none());
            assert!(effects.is_empty());
        }

        #[test]
        fn delete_connection_returns_delete_effect() {
            let mut state = create_test_state();
            enter_confirm_dialog(&mut state, InputMode::ConnectionSelector);
            let id = crate::domain::ConnectionId::new();
            state
                .confirm_dialog
                .open("", "", ConfirmIntent::DeleteConnection(id.clone()));

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::ConnectionSelector);
            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::DeleteConnection { .. }));
        }

        #[test]
        fn execute_write_sets_running_state_and_returns_effect() {
            let mut state = create_test_state();
            enter_confirm_dialog(&mut state, InputMode::CellEdit);
            state.session.dsn = Some("postgres://localhost/test".to_string());
            state.confirm_dialog.open(
                "",
                "",
                ConfirmIntent::ExecuteWrite {
                    sql: "UPDATE t SET x=1".to_string(),
                    blocked: false,
                },
            );

            let now = Instant::now();
            let effects = reduce_modal(&mut state, &Action::ConfirmDialogConfirm, now).unwrap();

            assert_eq!(state.input_mode(), InputMode::CellEdit);
            assert!(state.query.is_running());
            assert!(state.query.start_time().is_some());
            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::ExecuteWrite { .. }));
        }

        #[test]
        fn execute_write_no_dsn_sets_error() {
            let mut state = create_test_state();
            enter_confirm_dialog(&mut state, InputMode::Normal);
            state.session.dsn = None;
            state.confirm_dialog.open(
                "",
                "",
                ConfirmIntent::ExecuteWrite {
                    sql: "UPDATE t SET x=1".to_string(),
                    blocked: false,
                },
            );

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
            enter_confirm_dialog(&mut state, InputMode::Normal);
            state.confirm_dialog.open(
                "",
                "",
                ConfirmIntent::ExecuteWrite {
                    sql: "UPDATE t SET x=1".to_string(),
                    blocked: true,
                },
            );

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(effects.is_empty());
        }

        #[test]
        fn execute_write_blocked_confirm_clears_preview_state() {
            let mut state = create_test_state();
            enter_confirm_dialog(&mut state, InputMode::Normal);
            state.result_interaction.set_write_preview(
                crate::app::write_guardrails::WritePreview {
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
                },
            );
            state.query.set_delete_refresh_target(0, None, 1);
            state.confirm_dialog.open(
                "",
                "",
                ConfirmIntent::ExecuteWrite {
                    sql: "UPDATE t SET x=1".to_string(),
                    blocked: true,
                },
            );

            reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert!(state.result_interaction.pending_write_preview().is_none());
            assert!(state.query.pending_delete_refresh_target().is_none());
        }

        #[test]
        fn csv_export_returns_export_effect() {
            let mut state = create_test_state();
            enter_confirm_dialog(&mut state, InputMode::Normal);
            state.session.dsn = Some("postgres://localhost/test".to_string());
            state.confirm_dialog.open(
                "",
                "",
                ConfirmIntent::CsvExport {
                    export_query: "SELECT 1".to_string(),
                    file_name: "test.csv".to_string(),
                    row_count: Some(200_000),
                },
            );

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::ExportCsv { .. }));
        }

        #[test]
        fn disable_read_only_confirm_sets_read_only_false() {
            let mut state = create_test_state();
            state.session.read_only = true;
            enter_confirm_dialog(&mut state, InputMode::Normal);
            state
                .confirm_dialog
                .open("", "", ConfirmIntent::DisableReadOnly);

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert!(!state.session.read_only);
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(effects.is_empty());
        }

        #[test]
        fn none_intent_confirm_does_not_panic() {
            let mut state = create_test_state();
            enter_confirm_dialog(&mut state, InputMode::Normal);

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }
    }

    mod query_history_picker {
        use super::*;
        use crate::domain::ConnectionId;
        use crate::domain::query_history::{QueryHistoryEntry, QueryResultStatus};

        fn make_entry(query: &str, conn_id: &ConnectionId) -> QueryHistoryEntry {
            QueryHistoryEntry::new(
                query.to_string(),
                "2026-03-13T12:00:00Z".to_string(),
                conn_id.clone(),
                QueryResultStatus::Success,
                None,
            )
        }

        fn connected_state() -> AppState {
            let mut state = create_test_state();
            state.session.active_connection_id = Some(ConnectionId::from_string("test-conn"));
            state.runtime.project_name = "test-project".to_string();
            state
        }

        #[test]
        fn open_when_not_connected_is_noop() {
            let mut state = create_test_state();
            state.session.active_connection_id = None;

            let effects =
                reduce_modal(&mut state, &Action::OpenQueryHistoryPicker, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(effects.is_empty());
        }

        #[test]
        fn open_when_running_is_noop() {
            let mut state = connected_state();
            state.query.begin_running(Instant::now());

            let effects =
                reduce_modal(&mut state, &Action::OpenQueryHistoryPicker, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(effects.is_empty());
        }

        #[test]
        fn open_from_normal_sets_mode_and_emits_load_effect() {
            let mut state = connected_state();

            let effects =
                reduce_modal(&mut state, &Action::OpenQueryHistoryPicker, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::QueryHistoryPicker);
            assert_eq!(state.modal.return_destination(), InputMode::Normal);
            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::LoadQueryHistory { .. }));
        }

        #[test]
        fn close_restores_origin_mode() {
            let mut state = connected_state();
            state.modal.set_mode(InputMode::SqlModal);
            state.modal.push_mode(InputMode::QueryHistoryPicker);

            let effects =
                reduce_modal(&mut state, &Action::CloseQueryHistoryPicker, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::SqlModal);
            assert!(effects.is_empty());
        }

        #[test]
        fn loaded_stores_entries() {
            let mut state = connected_state();
            state.modal.set_mode(InputMode::QueryHistoryPicker);
            let conn_id = ConnectionId::from_string("test-conn");
            let entries = vec![make_entry("SELECT 1", &conn_id)];

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
            state.modal.set_mode(InputMode::QueryHistoryPicker);
            let stale_conn = ConnectionId::from_string("old-conn");
            let entries = vec![make_entry("SELECT 1", &stale_conn)];

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
            let conn_id = ConnectionId::from_string("test-conn");
            let entries = vec![make_entry("SELECT 1", &conn_id)];

            reduce_modal(
                &mut state,
                &Action::QueryHistoryLoaded(conn_id, entries),
                Instant::now(),
            )
            .unwrap();

            assert!(state.query_history_picker.entries.is_empty());
        }

        #[test]
        fn load_failed_sets_error_with_expiry() {
            let mut state = connected_state();
            let now = Instant::now();

            reduce_modal(
                &mut state,
                &Action::QueryHistoryLoadFailed("disk error".to_string()),
                now,
            )
            .unwrap();

            assert_eq!(state.messages.last_error.as_deref(), Some("disk error"));
            assert!(state.messages.expires_at.is_some());
        }

        #[test]
        fn filter_input_resets_selection() {
            let mut state = connected_state();
            state.query_history_picker.selected = 5;

            let effects = reduce_modal(
                &mut state,
                &Action::TextInput {
                    target: InputTarget::QueryHistoryFilter,
                    ch: 'a',
                },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.query_history_picker.selected, 0);
            assert_eq!(state.query_history_picker.filter_input.content(), "a");
            assert!(effects.is_empty());
        }

        fn enter_query_history(state: &mut AppState, origin: InputMode) {
            state.modal.set_mode(origin);
            state.modal.push_mode(InputMode::QueryHistoryPicker);
        }

        #[test]
        fn confirm_sets_cursor_to_char_count_not_byte_len() {
            let mut state = connected_state();
            enter_query_history(&mut state, InputMode::Normal);
            // 「SELECT 'あいう'」: 13 chars but 19 bytes
            let query = "SELECT '\u{3042}\u{3044}\u{3046}'".to_string();
            let expected_chars = query.chars().count(); // 13
            assert_ne!(query.len(), expected_chars); // sanity: bytes != chars
            let test_conn = ConnectionId::from_string("test-conn");
            state.query_history_picker.entries = vec![make_entry(&query, &test_conn)];

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
            enter_query_history(&mut state, InputMode::Normal);
            let test_conn = ConnectionId::from_string("test-conn");
            state.query_history_picker.entries =
                vec![make_entry("SELECT * FROM users", &test_conn)];
            state.query_history_picker.selected = 0;

            let effects = reduce_modal(
                &mut state,
                &Action::QueryHistoryConfirmSelection,
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::SqlModal);
            assert_eq!(state.sql_modal.content, "SELECT * FROM users");
            assert!(effects.is_empty());
        }

        #[test]
        fn confirm_from_sql_modal_overwrites_editor_content() {
            let mut state = connected_state();
            enter_query_history(&mut state, InputMode::SqlModal);
            state.sql_modal.content = "old query".to_string();
            let test_conn = ConnectionId::from_string("test-conn");
            state.query_history_picker.entries = vec![make_entry("new query", &test_conn)];
            state.query_history_picker.selected = 0;

            reduce_modal(
                &mut state,
                &Action::QueryHistoryConfirmSelection,
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::SqlModal);
            assert_eq!(state.sql_modal.content, "new query");
        }

        #[test]
        fn confirm_with_empty_entries_is_noop() {
            let mut state = connected_state();
            enter_query_history(&mut state, InputMode::Normal);

            reduce_modal(
                &mut state,
                &Action::QueryHistoryConfirmSelection,
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn select_next_increments() {
            let mut state = connected_state();
            let test_conn = ConnectionId::from_string("test-conn");
            state.query_history_picker.entries = vec![
                make_entry("SELECT 1", &test_conn),
                make_entry("SELECT 2", &test_conn),
            ];
            state.query_history_picker.selected = 0;

            reduce_modal(
                &mut state,
                &Action::ListSelect {
                    target: ListTarget::QueryHistory,
                    motion: ListMotion::Next,
                },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.query_history_picker.selected, 1);
        }

        #[test]
        fn select_next_clamps_at_end() {
            let mut state = connected_state();
            let test_conn = ConnectionId::from_string("test-conn");
            state.query_history_picker.entries = vec![make_entry("SELECT 1", &test_conn)];
            state.query_history_picker.selected = 0;

            reduce_modal(
                &mut state,
                &Action::ListSelect {
                    target: ListTarget::QueryHistory,
                    motion: ListMotion::Next,
                },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.query_history_picker.selected, 0);
        }

        #[test]
        fn select_previous_decrements() {
            let mut state = connected_state();
            state.query_history_picker.selected = 1;

            reduce_modal(
                &mut state,
                &Action::ListSelect {
                    target: ListTarget::QueryHistory,
                    motion: ListMotion::Previous,
                },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.query_history_picker.selected, 0);
        }
    }

    mod confirm_dialog_cancel {
        use super::confirm_dialog_confirm::enter_confirm_dialog;
        use super::*;

        #[test]
        fn quit_no_connection_restores_connection_setup_synchronously() {
            let mut state = create_test_state();
            enter_confirm_dialog(&mut state, InputMode::Normal);
            state
                .confirm_dialog
                .open("", "", ConfirmIntent::QuitNoConnection);

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogCancel, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::ConnectionSetup);
            assert!(effects.is_empty());
        }

        #[test]
        fn other_intents_cancel_returns_empty_effects() {
            let mut state = create_test_state();
            enter_confirm_dialog(&mut state, InputMode::CellEdit);
            state.confirm_dialog.open(
                "",
                "",
                ConfirmIntent::ExecuteWrite {
                    sql: "UPDATE t SET x=1".to_string(),
                    blocked: false,
                },
            );

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogCancel, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::CellEdit);
            assert!(effects.is_empty());
            assert!(state.result_interaction.pending_write_preview().is_none());
        }

        #[test]
        fn none_intent_cancel_does_not_panic() {
            let mut state = create_test_state();
            enter_confirm_dialog(&mut state, InputMode::Normal);

            let effects =
                reduce_modal(&mut state, &Action::ConfirmDialogCancel, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }
    }
}
