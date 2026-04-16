use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::shared::confirm_dialog::ConfirmIntent;
use crate::app::model::shared::flash_timer::FlashId;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::update::action::{
    Action, InputTarget, ListMotion, ListTarget, ScrollAmount, ScrollDirection, ScrollTarget,
};

fn scroll_help_by(state: &mut AppState, direction: ScrollDirection, delta: usize) {
    let max_scroll = state.ui.help_max_scroll();
    state.ui.help_scroll_offset =
        direction.clamp_vertical_offset(state.ui.help_scroll_offset, max_scroll, delta);
}

pub fn reduce_modal(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::OpenTablePicker => {
            state.modal.set_mode(InputMode::TablePicker);
            state.ui.table_picker.filter_input.clear();
            state.ui.table_picker.reset();
            Some(vec![])
        }
        Action::CloseTablePicker | Action::CloseCommandPalette | Action::Escape => {
            state.modal.set_mode(InputMode::Normal);
            Some(vec![])
        }
        Action::OpenCommandPalette => {
            state.modal.set_mode(InputMode::CommandPalette);
            state.ui.table_picker.reset();
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
            direction,
            amount,
        } => {
            match amount {
                ScrollAmount::Line => scroll_help_by(state, *direction, 1),
                ScrollAmount::ToStart => state.ui.help_scroll_offset = 0,
                ScrollAmount::ToEnd => state.ui.help_scroll_offset = state.ui.help_max_scroll(),
                ScrollAmount::HalfPage | ScrollAmount::FullPage => {
                    if let Some(delta) = amount.page_delta(state.ui.help_visible_rows()) {
                        scroll_help_by(state, *direction, delta);
                    }
                }
                ScrollAmount::ViewportTop
                | ScrollAmount::ViewportMiddle
                | ScrollAmount::ViewportBottom => {}
            }
            Some(vec![])
        }
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
            Some(vec![])
        }
        Action::CloseSqlModal => {
            state.modal.set_mode(InputMode::Normal);
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion_debounce = None;
            state.flash_timers.clear(FlashId::SqlModal);
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
            state.ui.er_picker.filter_input.insert_char(*c);
            state
                .ui
                .er_picker
                .filter_input
                .update_viewport(state.ui.er_picker.filter_visible_width);
            state.ui.er_picker.reset();
            Some(vec![])
        }
        Action::TextBackspace {
            target: InputTarget::ErFilter,
        } => {
            state.ui.er_picker.filter_input.backspace();
            state
                .ui
                .er_picker
                .filter_input
                .update_viewport(state.ui.er_picker.filter_visible_width);
            state.ui.er_picker.reset();
            Some(vec![])
        }
        Action::TextMoveCursor {
            target: InputTarget::ErFilter,
            direction: movement,
        } => {
            state.ui.er_picker.filter_input.move_cursor(*movement);
            state
                .ui
                .er_picker
                .filter_input
                .update_viewport(state.ui.er_picker.filter_visible_width);
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
            state.query_history_picker.entries.clone_from(entries);
            state.query_history_picker.selected = 0;
            state.query_history_picker.scroll_offset = 0;
            Some(vec![])
        }
        Action::QueryHistoryLoadFailed(e) => {
            if state.modal.active_mode() != InputMode::QueryHistoryPicker {
                return Some(vec![]);
            }
            state.messages.set_error_at(e.to_string(), now);
            Some(vec![])
        }
        Action::QueryHistoryAppendFailed(_) => Some(vec![]),
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
                    state.sql_modal.active_tab =
                        crate::app::model::sql_editor::modal::SqlModalTab::Sql;
                    state
                        .sql_modal
                        .set_status(crate::app::model::sql_editor::modal::SqlModalStatus::Normal);
                    state.sql_modal.editor.set_content(query);
                    state.sql_modal.reset_completion();
                }
                InputMode::SqlModal => {
                    state.sql_modal.active_tab =
                        crate::app::model::sql_editor::modal::SqlModalTab::Sql;
                    state.sql_modal.editor.set_content(query);
                    state
                        .sql_modal
                        .set_status(crate::app::model::sql_editor::modal::SqlModalStatus::Normal);
                    state.sql_modal.reset_completion();
                }
                _ => {}
            }
            Some(vec![])
        }

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
                None => Some(vec![]),
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
                Some(vec![])
            } else {
                state.modal.pop_mode();
                Some(vec![])
            }
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::confirm_dialog::ConfirmIntent;

    use std::time::Instant;

    fn create_test_state() -> AppState {
        AppState::new("test".to_string())
    }

    mod confirm_dialog {
        use super::*;

        pub(super) fn enter_confirm_dialog(state: &mut AppState, return_mode: InputMode) {
            state.modal.set_mode(return_mode);
            state.modal.push_mode(InputMode::ConfirmDialog);
        }

        mod confirm {
            use super::*;

            #[test]
            fn quit_no_connection_sets_should_quit() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state
                    .confirm_dialog
                    .open("", "", ConfirmIntent::QuitNoConnection);

                let effects =
                    reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now())
                        .unwrap();

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
                    .open("", "", ConfirmIntent::DeleteConnection(id));

                let effects =
                    reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now())
                        .unwrap();

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
                    reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now())
                        .unwrap();

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
                    reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now())
                        .unwrap();

                assert_eq!(state.input_mode(), InputMode::Normal);
                assert!(effects.is_empty());
            }

            #[test]
            fn execute_write_blocked_confirm_clears_preview_state() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state.result_interaction.set_write_preview(
                    crate::app::policy::write::write_guardrails::WritePreview {
                        operation:
                            crate::app::policy::write::write_guardrails::WriteOperation::Update,
                        sql: "UPDATE t SET x=1".to_string(),
                        target_summary:
                            crate::app::policy::write::write_guardrails::TargetSummary {
                                schema: "public".to_string(),
                                table: "t".to_string(),
                                key_values: vec![],
                            },
                        diff: vec![],
                        guardrail: crate::app::policy::write::write_guardrails::GuardrailDecision {
                            risk_level:
                                crate::app::policy::write::write_guardrails::RiskLevel::High,
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
                    reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now())
                        .unwrap();

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
                    reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now())
                        .unwrap();

                assert!(!state.session.read_only);
                assert_eq!(state.input_mode(), InputMode::Normal);
                assert!(effects.is_empty());
            }

            #[test]
            fn none_intent_confirm_does_not_panic() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);

                let effects =
                    reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now())
                        .unwrap();

                assert!(effects.is_empty());
            }
        }

        mod scroll {
            use super::*;

            fn state_with_scrollable_preview() -> AppState {
                let mut state = create_test_state();
                state.modal.set_mode(InputMode::ConfirmDialog);
                state.confirm_dialog.open(
                    "",
                    "",
                    ConfirmIntent::ExecuteWrite {
                        sql: "UPDATE t SET x=1".to_string(),
                        blocked: false,
                    },
                );
                state.confirm_dialog.preview_viewport_height = Some(10);
                state.confirm_dialog.preview_content_height = Some(25);
                state
            }

            #[test]
            fn down_increments_offset() {
                let mut state = state_with_scrollable_preview();

                reduce_modal(
                    &mut state,
                    &Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                );

                assert_eq!(state.confirm_dialog.preview_scroll, 1);
            }

            #[test]
            fn up_decrements_offset() {
                let mut state = state_with_scrollable_preview();
                state.confirm_dialog.preview_scroll = 5;

                reduce_modal(
                    &mut state,
                    &Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Up,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                );

                assert_eq!(state.confirm_dialog.preview_scroll, 4);
            }

            #[test]
            fn up_clamps_at_zero() {
                let mut state = state_with_scrollable_preview();
                state.confirm_dialog.preview_scroll = 0;

                reduce_modal(
                    &mut state,
                    &Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Up,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                );

                assert_eq!(state.confirm_dialog.preview_scroll, 0);
            }

            #[test]
            fn down_clamps_at_max() {
                let mut state = state_with_scrollable_preview();
                state.confirm_dialog.preview_scroll = 15;

                reduce_modal(
                    &mut state,
                    &Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                );

                assert_eq!(state.confirm_dialog.preview_scroll, 15);
            }

            #[test]
            fn open_resets_scroll_to_zero() {
                let mut state = create_test_state();
                state.confirm_dialog.preview_scroll = 10;

                state.confirm_dialog.open(
                    "",
                    "",
                    ConfirmIntent::ExecuteWrite {
                        sql: "test".to_string(),
                        blocked: false,
                    },
                );

                assert_eq!(state.confirm_dialog.preview_scroll, 0);
                assert!(state.confirm_dialog.preview_viewport_height.is_none());
                assert!(state.confirm_dialog.preview_content_height.is_none());
            }
        }

        mod cancel {
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

    mod query_history_picker {
        use super::*;
        use crate::app::model::shared::text_input::TextInputLike;
        use crate::app::ports::query_history::QueryHistoryError;
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

        fn enter_query_history(state: &mut AppState, origin: InputMode) {
            state.modal.set_mode(origin);
            state.modal.push_mode(InputMode::QueryHistoryPicker);
        }

        mod open_guards {
            use super::*;

            #[test]
            fn open_when_not_connected_is_noop() {
                let mut state = create_test_state();
                state.session.active_connection_id = None;

                let effects =
                    reduce_modal(&mut state, &Action::OpenQueryHistoryPicker, Instant::now())
                        .unwrap();

                assert_eq!(state.input_mode(), InputMode::Normal);
                assert!(effects.is_empty());
            }

            #[test]
            fn open_when_running_is_noop() {
                let mut state = connected_state();
                state.query.begin_running(Instant::now());

                let effects =
                    reduce_modal(&mut state, &Action::OpenQueryHistoryPicker, Instant::now())
                        .unwrap();

                assert_eq!(state.input_mode(), InputMode::Normal);
                assert!(effects.is_empty());
            }
        }

        mod lifecycle {
            use super::*;

            #[test]
            fn open_from_normal_sets_mode_and_emits_load_effect() {
                let mut state = connected_state();

                let effects =
                    reduce_modal(&mut state, &Action::OpenQueryHistoryPicker, Instant::now())
                        .unwrap();

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
                    reduce_modal(&mut state, &Action::CloseQueryHistoryPicker, Instant::now())
                        .unwrap();

                assert_eq!(state.input_mode(), InputMode::SqlModal);
                assert!(effects.is_empty());
            }
        }

        mod loading {
            use super::*;

            #[test]
            fn loaded_stores_entries() {
                let mut state = connected_state();
                state.modal.set_mode(InputMode::QueryHistoryPicker);
                let conn_id = ConnectionId::from_string("test-conn");
                let entries = vec![make_entry("SELECT 1", &conn_id)];

                let effects = reduce_modal(
                    &mut state,
                    &Action::QueryHistoryLoaded(conn_id, entries),
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
                state.modal.set_mode(InputMode::QueryHistoryPicker);
                let now = Instant::now();

                reduce_modal(
                    &mut state,
                    &Action::QueryHistoryLoadFailed(QueryHistoryError::IoError(
                        "disk error".to_string(),
                    )),
                    now,
                )
                .unwrap();

                assert_eq!(
                    state.messages.last_error.as_deref(),
                    Some("IO error: disk error")
                );
                assert!(state.messages.expires_at.is_some());
            }

            #[test]
            fn load_failed_ignored_when_picker_not_active() {
                let mut state = connected_state();
                let now = Instant::now();

                reduce_modal(
                    &mut state,
                    &Action::QueryHistoryLoadFailed(QueryHistoryError::IoError(
                        "stale error".to_string(),
                    )),
                    now,
                )
                .unwrap();

                assert!(state.messages.last_error.is_none());
            }

            #[test]
            fn append_failed_does_not_set_error() {
                let mut state = connected_state();
                let now = Instant::now();

                let effects = reduce_modal(
                    &mut state,
                    &Action::QueryHistoryAppendFailed(QueryHistoryError::IoError(
                        "write error".to_string(),
                    )),
                    now,
                )
                .unwrap();

                assert!(state.messages.last_error.is_none());
                assert!(effects.is_empty());
            }
        }

        mod filter_and_selection {
            use super::*;

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

        mod confirm_selection {
            use super::*;

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

                assert_eq!(state.sql_modal.editor.cursor(), expected_chars);
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
                assert_eq!(state.sql_modal.editor.content(), "SELECT * FROM users");
                assert!(matches!(
                    state.sql_modal.status(),
                    crate::app::model::sql_editor::modal::SqlModalStatus::Normal
                ));
                assert!(effects.is_empty());
            }

            #[test]
            fn confirm_from_sql_modal_overwrites_editor_content() {
                let mut state = connected_state();
                enter_query_history(&mut state, InputMode::SqlModal);
                state.sql_modal.editor.set_content("old query".to_string());
                state
                    .sql_modal
                    .set_status(crate::app::model::sql_editor::modal::SqlModalStatus::Editing);
                state.sql_modal.completion.visible = true;
                state.sql_modal.completion.candidates = vec![
                    crate::app::model::sql_editor::completion::CompletionCandidate {
                        text: "stale".to_string(),
                        kind: crate::app::model::sql_editor::completion::CompletionKind::Keyword,
                        score: 1,
                    },
                ];
                state.sql_modal.completion.selected_index = 3;
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
                assert_eq!(state.sql_modal.editor.content(), "new query");
                assert!(matches!(
                    state.sql_modal.status(),
                    crate::app::model::sql_editor::modal::SqlModalStatus::Normal
                ));
                assert!(!state.sql_modal.completion.visible);
                assert!(state.sql_modal.completion.candidates.is_empty());
                assert_eq!(state.sql_modal.completion.selected_index, 0);
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
        }
    }
}
