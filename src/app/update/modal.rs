use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::confirm_dialog::ConfirmIntent;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{
    Action, InputTarget, ListMotion, ListTarget, ModalKind, ScrollAmount, ScrollDirection,
    ScrollTarget,
};

fn scroll_help_by(state: &mut AppState, direction: ScrollDirection, delta: usize) {
    let max_scroll = state.ui.help_max_scroll();
    state
        .ui
        .set_help_scroll_offset(direction.clamp_vertical_offset(
            state.ui.help_scroll_offset(),
            max_scroll,
            delta,
        ));
}

pub fn reduce_modal(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::OpenModal(ModalKind::TablePicker) => {
            state.modal.set_mode(InputMode::TablePicker);
            state.ui.table_picker.clear_filter_and_reset();
            Some(vec![])
        }
        Action::CloseModal(ModalKind::TablePicker)
        | Action::CloseModal(ModalKind::CommandPalette)
        | Action::Escape => {
            state.modal.set_mode(InputMode::Normal);
            Some(vec![])
        }
        Action::OpenModal(ModalKind::CommandPalette) => {
            state.modal.set_mode(InputMode::CommandPalette);
            // Command palette currently reuses the generic picker selection state.
            state.ui.table_picker.reset();
            Some(vec![])
        }
        Action::ToggleModal(ModalKind::Help) => {
            if state.modal.active_mode() == InputMode::Help {
                state.modal.set_mode(InputMode::Normal);
                state.ui.set_help_scroll_offset(0);
            } else {
                state.modal.set_mode(InputMode::Help);
            }
            Some(vec![])
        }
        Action::CloseModal(ModalKind::Help) => {
            state.modal.set_mode(InputMode::Normal);
            state.ui.set_help_scroll_offset(0);
            Some(vec![])
        }
        Action::Scroll {
            target: ScrollTarget::Help,
            direction,
            amount,
        } => {
            match amount {
                ScrollAmount::Line => scroll_help_by(state, *direction, 1),
                ScrollAmount::ToStart => state.ui.set_help_scroll_offset(0),
                ScrollAmount::ToEnd => state.ui.set_help_scroll_offset(state.ui.help_max_scroll()),
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
            state.confirm_dialog.scroll_preview(*direction);
            Some(vec![])
        }
        Action::CloseModal(ModalKind::SqlModal) => {
            state.modal.set_mode(InputMode::Normal);
            state.sql_modal.cleanup_on_close();
            state.flash_timers.clear(FlashId::SqlModal);
            Some(vec![])
        }
        Action::OpenModal(ModalKind::ErTablePicker) => {
            if state.session.metadata().is_none() {
                state.ui.pending_er_picker = true;
                state.set_success("Waiting for metadata...".to_string());
                return Some(vec![]);
            }
            state.ui.pending_er_picker = false;
            state.ui.er_selected_tables.clear();
            state.modal.set_mode(InputMode::ErTablePicker);
            state.ui.er_picker.clear_filter_and_reset();
            Some(vec![])
        }
        Action::CloseModal(ModalKind::ErTablePicker) => {
            state.modal.set_mode(InputMode::Normal);
            state.ui.er_picker.clear_filter();
            state.ui.er_selected_tables.clear();
            state.ui.pending_er_picker = false;
            Some(vec![])
        }
        Action::TextInput {
            target: InputTarget::ErFilter,
            ch: c,
        } => {
            state.ui.er_picker.insert_filter_char(*c);
            Some(vec![])
        }
        Action::TextBackspace {
            target: InputTarget::ErFilter,
        } => {
            state.ui.er_picker.backspace_filter();
            Some(vec![])
        }
        Action::TextMoveCursor {
            target: InputTarget::ErFilter,
            direction: movement,
        } => {
            state.ui.er_picker.move_filter_cursor(*movement);
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
            state
                .er_preparation
                .set_targets(state.ui.er_selected_tables.iter().cloned().collect());
            state.modal.set_mode(InputMode::Normal);
            state.ui.er_picker.clear_filter();
            state.ui.er_selected_tables.clear();
            Some(vec![Effect::DispatchActions(vec![Action::ErOpenDiagram])])
        }
        Action::OpenModal(ModalKind::QueryHistoryPicker) => {
            if state.session.active_connection_id().is_none() {
                return Some(vec![]);
            }
            if state.query.is_running() {
                return Some(vec![]);
            }
            if state.modal.active_mode() == InputMode::ConfirmDialog {
                return Some(vec![]);
            }
            if state.sql_modal.completion().visible
                && !state.sql_modal.completion().candidates.is_empty()
            {
                return Some(vec![]);
            }

            state.query_history_picker.reset();
            state.modal.push_mode(InputMode::QueryHistoryPicker);

            let conn_id = state.session.active_connection_id().unwrap();
            Some(vec![Effect::LoadQueryHistory {
                project_name: state.runtime.project_name().to_string(),
                connection_id: conn_id.clone(),
            }])
        }
        Action::CloseModal(ModalKind::QueryHistoryPicker) => {
            state.modal.pop_mode();
            state.query_history_picker.reset();
            Some(vec![])
        }
        Action::QueryHistoryLoaded(conn_id, entries) => {
            if state.modal.active_mode() != InputMode::QueryHistoryPicker {
                return Some(vec![]);
            }
            if state.session.active_connection_id() != Some(conn_id) {
                return Some(vec![]);
            }
            state.query_history_picker.replace_entries(entries);
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
            state.query_history_picker.insert_filter_char(*c);
            Some(vec![])
        }
        Action::TextBackspace {
            target: InputTarget::QueryHistoryFilter,
        } => {
            state.query_history_picker.backspace_filter();
            Some(vec![])
        }
        Action::ListSelect {
            target: ListTarget::QueryHistory,
            motion: ListMotion::Next,
        } => {
            state.query_history_picker.select_next();
            Some(vec![])
        }
        Action::ListSelect {
            target: ListTarget::QueryHistory,
            motion: ListMotion::Previous,
        } => {
            state.query_history_picker.select_previous();
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
                    state.sql_modal.load_query_from_history(query);
                }
                InputMode::SqlModal => {
                    state.sql_modal.load_query_from_history(query);
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
                    if let Some(dsn) = state.session.dsn() {
                        state.query.begin_running(now);
                        Some(vec![Effect::ExecuteWrite {
                            dsn: dsn.to_string(),
                            query: sql,
                            read_only: state.session.is_read_only(),
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
                    state.session.disable_read_only();
                    Some(vec![])
                }
                Some(ConfirmIntent::CsvExport {
                    export_query,
                    file_name,
                    row_count,
                }) => {
                    if let Some(dsn) = state.session.dsn() {
                        Some(vec![Effect::ExportCsv {
                            dsn: dsn.to_string(),
                            query: export_query,
                            file_name,
                            row_count,
                            read_only: state.session.is_read_only(),
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
                if !state.connections().is_empty() || state.session.dsn().is_some() {
                    state.connection_setup.set_first_run(false);
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
    use std::sync::Arc;

    use super::*;
    use crate::model::shared::confirm_dialog::ConfirmIntent;

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
                state.session.set_dsn_for_test("postgres://localhost/test");
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
                state.session.clear_dsn_for_test();
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
                assert_eq!(state.messages.last_error(), Some("No active connection"));
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
                    crate::policy::write::write_guardrails::WritePreview {
                        operation: crate::policy::write::write_guardrails::WriteOperation::Update,
                        sql: "UPDATE t SET x=1".to_string(),
                        target_summary: crate::policy::write::write_guardrails::TargetSummary {
                            schema: "public".to_string(),
                            table: "t".to_string(),
                            key_values: vec![],
                        },
                        diff: vec![],
                        guardrail: crate::policy::write::write_guardrails::GuardrailDecision {
                            risk_level: crate::policy::write::write_guardrails::RiskLevel::High,
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
                state.session.set_dsn_for_test("postgres://localhost/test");
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
                state.session.enable_read_only();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state
                    .confirm_dialog
                    .open("", "", ConfirmIntent::DisableReadOnly);

                let effects =
                    reduce_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now())
                        .unwrap();

                assert!(!state.session.is_read_only());
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
                state
                    .confirm_dialog
                    .apply_preview_metrics(Some(10), Some(25), 0);
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

                assert_eq!(state.confirm_dialog.preview_scroll(), 1);
            }

            #[test]
            fn up_decrements_offset() {
                let mut state = state_with_scrollable_preview();
                state
                    .confirm_dialog
                    .apply_preview_metrics(Some(10), Some(25), 5);

                reduce_modal(
                    &mut state,
                    &Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Up,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                );

                assert_eq!(state.confirm_dialog.preview_scroll(), 4);
            }

            #[test]
            fn up_clamps_at_zero() {
                let mut state = state_with_scrollable_preview();
                state
                    .confirm_dialog
                    .apply_preview_metrics(Some(10), Some(25), 0);

                reduce_modal(
                    &mut state,
                    &Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Up,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                );

                assert_eq!(state.confirm_dialog.preview_scroll(), 0);
            }

            #[test]
            fn down_clamps_at_max() {
                let mut state = state_with_scrollable_preview();
                state
                    .confirm_dialog
                    .apply_preview_metrics(Some(10), Some(25), 15);

                reduce_modal(
                    &mut state,
                    &Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                );

                assert_eq!(state.confirm_dialog.preview_scroll(), 15);
            }

            #[test]
            fn open_resets_scroll_to_zero() {
                let mut state = create_test_state();
                state
                    .confirm_dialog
                    .apply_preview_metrics(Some(10), Some(25), 10);

                state.confirm_dialog.open(
                    "",
                    "",
                    ConfirmIntent::ExecuteWrite {
                        sql: "test".to_string(),
                        blocked: false,
                    },
                );

                assert_eq!(state.confirm_dialog.preview_scroll(), 0);
                assert!(state.confirm_dialog.preview_viewport_height().is_none());
                assert!(state.confirm_dialog.preview_content_height().is_none());
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
        use crate::domain::ConnectionId;
        use crate::domain::query_history::{QueryHistoryEntry, QueryResultStatus};
        use crate::model::shared::text_input::TextInputLike;
        use crate::ports::outbound::query_history::QueryHistoryError;

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
            state
                .session
                .set_active_connection_id_for_test(Some(ConnectionId::from_string("test-conn")));
            state
                .runtime
                .set_project_name_for_test("test-project".to_string());
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
                state.session.set_active_connection_id_for_test(None);

                let effects = reduce_modal(
                    &mut state,
                    &Action::OpenModal(ModalKind::QueryHistoryPicker),
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.input_mode(), InputMode::Normal);
                assert!(effects.is_empty());
            }

            #[test]
            fn open_when_running_is_noop() {
                let mut state = connected_state();
                state.query.begin_running(Instant::now());

                let effects = reduce_modal(
                    &mut state,
                    &Action::OpenModal(ModalKind::QueryHistoryPicker),
                    Instant::now(),
                )
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

                let effects = reduce_modal(
                    &mut state,
                    &Action::OpenModal(ModalKind::QueryHistoryPicker),
                    Instant::now(),
                )
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

                let effects = reduce_modal(
                    &mut state,
                    &Action::CloseModal(ModalKind::QueryHistoryPicker),
                    Instant::now(),
                )
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

                assert_eq!(state.query_history_picker.entries().len(), 1);
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

                assert!(state.query_history_picker.entries().is_empty());
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

                assert!(state.query_history_picker.entries().is_empty());
            }

            #[test]
            fn load_failed_sets_error_with_expiry() {
                let mut state = connected_state();
                state.modal.set_mode(InputMode::QueryHistoryPicker);
                let now = Instant::now();

                reduce_modal(
                    &mut state,
                    &Action::QueryHistoryLoadFailed(QueryHistoryError::Io(Arc::new(
                        std::io::Error::other("disk error"),
                    ))),
                    now,
                )
                .unwrap();

                assert_eq!(state.messages.last_error(), Some("IO error: disk error"));
                assert!(state.messages.expires_at().is_some());
            }

            #[test]
            fn load_failed_ignored_when_picker_not_active() {
                let mut state = connected_state();
                let now = Instant::now();

                reduce_modal(
                    &mut state,
                    &Action::QueryHistoryLoadFailed(QueryHistoryError::Io(Arc::new(
                        std::io::Error::other("stale error"),
                    ))),
                    now,
                )
                .unwrap();

                assert!(state.messages.last_error().is_none());
            }

            #[test]
            fn append_failed_does_not_set_error() {
                let mut state = connected_state();
                let now = Instant::now();

                let effects = reduce_modal(
                    &mut state,
                    &Action::QueryHistoryAppendFailed(QueryHistoryError::Io(Arc::new(
                        std::io::Error::other("write error"),
                    ))),
                    now,
                )
                .unwrap();

                assert!(state.messages.last_error().is_none());
                assert!(effects.is_empty());
            }
        }

        mod filter_and_selection {
            use super::*;

            #[test]
            fn filter_input_resets_selection() {
                let mut state = connected_state();
                state.query_history_picker.set_selection_for_test(5);

                let effects = reduce_modal(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::QueryHistoryFilter,
                        ch: 'a',
                    },
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.query_history_picker.selected(), 0);
                assert_eq!(state.query_history_picker.filter_input().content(), "a");
                assert!(effects.is_empty());
            }

            #[test]
            fn select_next_increments() {
                let mut state = connected_state();
                let test_conn = ConnectionId::from_string("test-conn");
                state.query_history_picker.replace_entries(&[
                    make_entry("SELECT 1", &test_conn),
                    make_entry("SELECT 2", &test_conn),
                ]);

                reduce_modal(
                    &mut state,
                    &Action::ListSelect {
                        target: ListTarget::QueryHistory,
                        motion: ListMotion::Next,
                    },
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.query_history_picker.selected(), 1);
            }

            #[test]
            fn select_next_clamps_at_end() {
                let mut state = connected_state();
                let test_conn = ConnectionId::from_string("test-conn");
                state
                    .query_history_picker
                    .replace_entries(&[make_entry("SELECT 1", &test_conn)]);

                reduce_modal(
                    &mut state,
                    &Action::ListSelect {
                        target: ListTarget::QueryHistory,
                        motion: ListMotion::Next,
                    },
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.query_history_picker.selected(), 0);
            }

            #[test]
            fn select_previous_decrements() {
                let mut state = connected_state();
                state.query_history_picker.set_selection_for_test(1);

                reduce_modal(
                    &mut state,
                    &Action::ListSelect {
                        target: ListTarget::QueryHistory,
                        motion: ListMotion::Previous,
                    },
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.query_history_picker.selected(), 0);
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
                state
                    .query_history_picker
                    .replace_entries(&[make_entry(&query, &test_conn)]);

                reduce_modal(
                    &mut state,
                    &Action::QueryHistoryConfirmSelection,
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.sql_modal.editor().cursor(), expected_chars);
            }

            #[test]
            fn confirm_from_normal_opens_sql_modal_with_query() {
                let mut state = connected_state();
                enter_query_history(&mut state, InputMode::Normal);
                let test_conn = ConnectionId::from_string("test-conn");
                state
                    .query_history_picker
                    .replace_entries(&[make_entry("SELECT * FROM users", &test_conn)]);

                let effects = reduce_modal(
                    &mut state,
                    &Action::QueryHistoryConfirmSelection,
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.input_mode(), InputMode::SqlModal);
                assert_eq!(state.sql_modal.editor().content(), "SELECT * FROM users");
                assert!(matches!(
                    state.sql_modal.status(),
                    crate::model::sql_editor::modal::SqlModalStatus::Normal
                ));
                assert!(effects.is_empty());
            }

            #[test]
            fn confirm_from_sql_modal_overwrites_editor_content() {
                let mut state = connected_state();
                enter_query_history(&mut state, InputMode::SqlModal);
                state
                    .sql_modal
                    .set_editor_content_for_test("old query".to_string());
                state
                    .sql_modal
                    .set_status_for_test(crate::model::sql_editor::modal::SqlModalStatus::Editing);
                state.sql_modal.completion_mut_for_test().visible = true;
                state.sql_modal.completion_mut_for_test().candidates =
                    vec![crate::model::sql_editor::completion::CompletionCandidate {
                        text: "stale".to_string(),
                        kind: crate::model::sql_editor::completion::CompletionKind::Keyword,
                        score: 1,
                    }];
                state.sql_modal.completion_mut_for_test().selected_index = 3;
                let test_conn = ConnectionId::from_string("test-conn");
                state
                    .query_history_picker
                    .replace_entries(&[make_entry("new query", &test_conn)]);

                reduce_modal(
                    &mut state,
                    &Action::QueryHistoryConfirmSelection,
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.input_mode(), InputMode::SqlModal);
                assert_eq!(state.sql_modal.editor().content(), "new query");
                assert!(matches!(
                    state.sql_modal.status(),
                    crate::model::sql_editor::modal::SqlModalStatus::Normal
                ));
                assert!(!state.sql_modal.completion().visible);
                assert!(state.sql_modal.completion().candidates.is_empty());
                assert_eq!(state.sql_modal.completion().selected_index, 0);
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
