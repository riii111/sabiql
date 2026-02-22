//! Pure reducer: state transitions only, no I/O.
//!
//! # Purity Rules
//!
//! The reducer MUST NOT:
//! - Call `Instant::now()` (time is passed as `now` parameter)
//! - Perform I/O operations
//! - Spawn async tasks
//!
//! This keeps the reducer testable without mocking time or I/O.

use std::time::Instant;

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::focused_pane::FocusedPane;
use crate::app::input_mode::InputMode;
use crate::app::reducers::{
    reduce_connection, reduce_er, reduce_metadata, reduce_modal, reduce_navigation, reduce_query,
    reduce_sql_modal,
};
use crate::app::state::AppState;

pub fn reduce(state: &mut AppState, action: Action, now: Instant) -> Vec<Effect> {
    // Mark dirty for all state-changing actions (except None and Render)
    let should_mark_dirty = !matches!(action, Action::None | Action::Render);

    let effects = reduce_inner(state, action, now);

    if should_mark_dirty {
        state.mark_dirty();
    }

    effects
}

fn reduce_inner(state: &mut AppState, action: Action, now: Instant) -> Vec<Effect> {
    if let Some(effects) = reduce_connection(state, &action, now) {
        return effects;
    }
    if let Some(effects) = reduce_modal(state, &action, now) {
        return effects;
    }
    if let Some(effects) = reduce_navigation(state, &action, now) {
        return effects;
    }
    if let Some(effects) = reduce_sql_modal(state, &action, now) {
        return effects;
    }
    if let Some(effects) = reduce_metadata(state, &action, now) {
        return effects;
    }
    if let Some(effects) = reduce_er(state, &action, now) {
        return effects;
    }
    if let Some(effects) = reduce_query(state, &action, now) {
        return effects;
    }

    match action {
        Action::None => vec![],
        Action::Quit => {
            state.should_quit = true;
            vec![]
        }
        Action::Resize(_w, h) => {
            state.ui.terminal_height = h;
            vec![]
        }
        Action::Render => {
            vec![Effect::Render]
        }

        Action::ConfirmSelection => {
            let mut effects = Vec::new();

            if state.ui.input_mode == InputMode::TablePicker {
                let filtered = state.filtered_tables();
                if let Some(table) = filtered.get(state.ui.picker_selected).cloned() {
                    let schema = table.schema.clone();
                    let table_name = table.name.clone();
                    state.cache.current_table = Some(table.qualified_name());
                    state.cache.table_detail = None;
                    state.ui.input_mode = InputMode::Normal;
                    state.cell_edit.clear();
                    state.pending_write_preview = None;

                    state.cache.selection_generation += 1;
                    let current_gen = state.cache.selection_generation;

                    if let Some(dsn) = &state.runtime.dsn {
                        effects.push(Effect::FetchTableDetail {
                            dsn: dsn.clone(),
                            schema: schema.clone(),
                            table: table_name.clone(),
                            generation: current_gen,
                        });
                    }
                    effects.push(Effect::DispatchActions(vec![Action::ExecutePreview {
                        schema,
                        table: table_name,
                        generation: current_gen,
                    }]));
                }
            } else if state.ui.input_mode == InputMode::Normal {
                // Open error modal if connection error exists (from any pane)
                if state.connection_error.error_info.is_some() {
                    state.ui.input_mode = InputMode::ConnectionError;
                    return effects;
                }

                if state.ui.focused_pane != FocusedPane::Explorer {
                    return effects;
                }

                let tables = state.tables();
                if let Some(table) = tables.get(state.ui.explorer_selected).cloned() {
                    let schema = table.schema.clone();
                    let table_name = table.name.clone();
                    state.cache.current_table = Some(table.qualified_name());
                    state.cache.table_detail = None;
                    state.cell_edit.clear();
                    state.pending_write_preview = None;

                    state.cache.selection_generation += 1;
                    let current_gen = state.cache.selection_generation;

                    if let Some(dsn) = &state.runtime.dsn {
                        effects.push(Effect::FetchTableDetail {
                            dsn: dsn.clone(),
                            schema: schema.clone(),
                            table: table_name.clone(),
                            generation: current_gen,
                        });
                    }
                    effects.push(Effect::DispatchActions(vec![Action::ExecutePreview {
                        schema,
                        table: table_name,
                        generation: current_gen,
                    }]));
                }
            } else if state.ui.input_mode == InputMode::CommandPalette {
                use crate::app::palette::palette_action_for_index;

                let cmd_action = palette_action_for_index(state.ui.picker_selected);
                state.ui.input_mode = InputMode::Normal;
                let mut sub_effects = reduce(state, cmd_action, now);
                effects.append(&mut sub_effects);
            }

            effects
        }

        // Handled by sub-reducers
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_state() -> AppState {
        AppState::new("test_project".to_string())
    }

    mod pure_actions {
        use super::*;
        use rstest::rstest;

        #[test]
        fn quit_sets_should_quit_and_returns_no_effects() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::Quit, now);

            assert!(state.should_quit);
            assert!(effects.is_empty());
        }

        #[test]
        fn toggle_focus_returns_no_effects() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ToggleFocus, now);

            assert!(state.ui.focus_mode);
            assert!(effects.is_empty());
        }

        #[test]
        fn resize_updates_terminal_height() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::Resize(100, 50), now);

            assert_eq!(state.ui.terminal_height, 50);
            assert!(effects.is_empty());
        }

        #[test]
        fn render_returns_render_effect() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::Render, now);

            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::Render));
        }

        #[rstest]
        #[case(Action::SelectFirst)]
        #[case(Action::SelectLast)]
        #[case(Action::SelectNext)]
        #[case(Action::SelectPrevious)]
        fn selection_on_empty_tables_keeps_none(#[case] action: Action) {
            let mut state = create_test_state();
            state.ui.focused_pane = FocusedPane::Explorer;
            state.ui.explorer_selected = 0;
            let now = Instant::now();

            let _ = reduce(&mut state, action, now);

            assert_eq!(state.ui.explorer_selected, 0);
        }
    }

    mod scroll_actions {
        use super::*;

        #[test]
        fn result_scroll_up_decrements_offset() {
            let mut state = create_test_state();
            state.ui.result_scroll_offset = 5;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ResultScrollUp, now);

            assert_eq!(state.ui.result_scroll_offset, 4);
            assert!(effects.is_empty());
        }

        #[test]
        fn result_scroll_up_saturates_at_zero() {
            let mut state = create_test_state();
            state.ui.result_scroll_offset = 0;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ResultScrollUp, now);

            assert_eq!(state.ui.result_scroll_offset, 0);
            assert!(effects.is_empty());
        }

        #[test]
        fn result_scroll_top_resets_to_zero() {
            let mut state = create_test_state();
            state.ui.result_scroll_offset = 10;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ResultScrollTop, now);

            assert_eq!(state.ui.result_scroll_offset, 0);
            assert!(effects.is_empty());
        }
    }

    mod modal_toggles {
        use super::*;

        #[test]
        fn open_table_picker_sets_mode_and_clears_filter() {
            let mut state = create_test_state();
            state.ui.filter_input = "test".to_string();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::OpenTablePicker, now);

            assert_eq!(state.ui.input_mode, InputMode::TablePicker);
            assert!(state.ui.filter_input.is_empty());
            assert_eq!(state.ui.picker_selected, 0);
            assert!(effects.is_empty());
        }

        #[test]
        fn close_table_picker_returns_to_normal() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::TablePicker;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::CloseTablePicker, now);

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert!(effects.is_empty());
        }

        #[test]
        fn open_help_toggles_help_mode() {
            let mut state = create_test_state();
            let now = Instant::now();

            // First open
            let effects = reduce(&mut state, Action::OpenHelp, now);
            assert_eq!(state.ui.input_mode, InputMode::Help);
            assert!(effects.is_empty());

            // Toggle back to normal
            let effects = reduce(&mut state, Action::OpenHelp, now);
            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert!(effects.is_empty());
        }
    }

    mod sql_modal_debounce {
        use super::*;
        use std::time::Duration;

        #[test]
        fn sql_modal_input_sets_debounce_state() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::SqlModal;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::SqlModalInput('a'), now);

            assert_eq!(state.sql_modal.content, "a");
            assert_eq!(state.sql_modal.cursor, 1);
            assert!(effects.is_empty());
            assert!(state.sql_modal.completion_debounce.is_some());
        }

        #[test]
        fn sql_modal_backspace_sets_debounce_state() {
            let mut state = create_test_state();
            state.sql_modal.content = "ab".to_string();
            state.sql_modal.cursor = 2;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::SqlModalBackspace, now);

            assert_eq!(state.sql_modal.content, "a");
            assert_eq!(state.sql_modal.cursor, 1);
            assert!(effects.is_empty());
            assert!(state.sql_modal.completion_debounce.is_some());
        }

        #[test]
        fn debounce_state_uses_provided_now() {
            let mut state = create_test_state();
            let now = Instant::now();

            let _ = reduce(&mut state, Action::SqlModalInput('x'), now);

            let expected = now + Duration::from_millis(100);
            assert_eq!(state.sql_modal.completion_debounce, Some(expected));
        }
    }

    mod completion_ui {
        use super::*;
        use crate::app::sql_modal_context::{CompletionCandidate, CompletionKind};

        fn make_candidate(text: &str) -> CompletionCandidate {
            CompletionCandidate {
                text: text.to_string(),
                kind: CompletionKind::Table,
                score: 0,
            }
        }

        #[test]
        fn completion_next_wraps_around() {
            let mut state = create_test_state();
            state.sql_modal.completion.candidates = vec![make_candidate("a"), make_candidate("b")];
            state.sql_modal.completion.selected_index = 1;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::CompletionNext, now);

            assert_eq!(state.sql_modal.completion.selected_index, 0);
            assert!(effects.is_empty());
        }

        #[test]
        fn completion_prev_wraps_around() {
            let mut state = create_test_state();
            state.sql_modal.completion.candidates = vec![make_candidate("a"), make_candidate("b")];
            state.sql_modal.completion.selected_index = 0;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::CompletionPrev, now);

            assert_eq!(state.sql_modal.completion.selected_index, 1);
            assert!(effects.is_empty());
        }
    }

    mod response_handlers {
        use super::*;
        use crate::app::connection_error::ConnectionErrorInfo;
        use crate::domain::{DatabaseMetadata, MetadataState, TableSummary};

        #[test]
        fn metadata_loaded_with_empty_tables_selects_none() {
            let mut state = create_test_state();
            state.ui.explorer_selected = 5;
            let metadata = DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: Instant::now(),
            };
            let now = Instant::now();

            let _ = reduce(&mut state, Action::MetadataLoaded(Box::new(metadata)), now);

            assert!(state.cache.metadata.is_some());
            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn metadata_loaded_with_tables_selects_first() {
            let mut state = create_test_state();
            state.ui.explorer_selected = 3;
            let metadata = DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![TableSummary::new(
                    "public".to_string(),
                    "users".to_string(),
                    None,
                    false,
                )],
                fetched_at: Instant::now(),
            };
            let now = Instant::now();

            let _ = reduce(&mut state, Action::MetadataLoaded(Box::new(metadata)), now);

            assert!(state.cache.metadata.is_some());
            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn metadata_failed_opens_error_modal_automatically() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::MetadataFailed("psql: error: connection refused".to_string()),
                now,
            );

            assert!(matches!(state.cache.state, MetadataState::Error(_)));
            assert_eq!(state.ui.input_mode, InputMode::ConnectionError);
            assert!(state.connection_error.error_info.is_some());
            assert!(effects.is_empty());
        }

        #[test]
        fn enter_with_error_info_opens_modal() {
            let mut state = create_test_state();
            state
                .connection_error
                .set_error(ConnectionErrorInfo::new("error"));
            state.ui.focused_pane = FocusedPane::Result; // Any pane works
            let now = Instant::now();

            reduce(&mut state, Action::ConfirmSelection, now);

            assert_eq!(state.ui.input_mode, InputMode::ConnectionError);
        }
    }

    mod connection_error_actions {
        use super::*;
        use crate::app::connection_error::{ConnectionErrorInfo, ConnectionErrorKind};
        use crate::domain::MetadataState;

        fn state_with_error() -> AppState {
            let mut state = create_test_state();
            let info = ConnectionErrorInfo::with_kind(
                ConnectionErrorKind::HostUnreachable,
                "psql: error: could not translate host",
            );
            state.connection_error.set_error(info);
            state.ui.input_mode = InputMode::ConnectionError;
            state
        }

        #[test]
        fn close_keeps_error_info_for_reopen() {
            let mut state = state_with_error();
            state.connection_error.details_expanded = true;
            state.connection_error.scroll_offset = 5;
            let now = Instant::now();

            reduce(&mut state, Action::CloseConnectionError, now);

            // error_info is kept so Enter can re-open modal
            assert!(state.connection_error.error_info.is_some());
            assert_eq!(state.ui.input_mode, InputMode::Normal);
            // UI state is reset
            assert!(!state.connection_error.details_expanded);
            assert_eq!(state.connection_error.scroll_offset, 0);
        }

        #[test]
        fn close_clears_copied_feedback() {
            let mut state = state_with_error();
            let now = Instant::now();
            state.connection_error.mark_copied_at(now);
            assert!(state.connection_error.is_copied_visible_at(now));

            reduce(&mut state, Action::CloseConnectionError, now);

            // Copied feedback is cleared on close
            assert!(!state.connection_error.is_copied_visible_at(now));
        }

        #[test]
        fn reopen_modal_after_close_shows_same_error() {
            let mut state = state_with_error();
            state.cache.state = MetadataState::Error("error".to_string());
            state.ui.focused_pane = FocusedPane::Explorer;
            let now = Instant::now();

            // Close modal
            reduce(&mut state, Action::CloseConnectionError, now);
            assert_eq!(state.ui.input_mode, InputMode::Normal);

            // Re-open with Enter
            reduce(&mut state, Action::ConfirmSelection, now);
            assert_eq!(state.ui.input_mode, InputMode::ConnectionError);
            assert!(state.connection_error.error_info.is_some());
        }

        #[test]
        fn toggle_details_flips_expanded_state() {
            let mut state = state_with_error();
            let now = Instant::now();
            assert!(!state.connection_error.details_expanded);

            reduce(&mut state, Action::ToggleConnectionErrorDetails, now);
            assert!(state.connection_error.details_expanded);

            reduce(&mut state, Action::ToggleConnectionErrorDetails, now);
            assert!(!state.connection_error.details_expanded);
        }

        #[test]
        fn copy_returns_clipboard_effect() {
            let mut state = state_with_error();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::CopyConnectionError, now);

            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::CopyToClipboard { .. }));
        }

        #[test]
        fn copied_marks_feedback_visible() {
            let mut state = state_with_error();
            let now = Instant::now();

            reduce(&mut state, Action::ConnectionErrorCopied, now);

            assert!(state.connection_error.is_copied_visible_at(now));
        }
    }

    mod confirm_selection_safety {
        use super::*;
        use crate::domain::{DatabaseMetadata, Table, TableSummary};

        fn stale_table_detail() -> Table {
            Table {
                schema: "public".to_string(),
                name: "old_table".to_string(),
                owner: None,
                columns: vec![],
                primary_key: None,
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            }
        }

        fn users_metadata(now: Instant) -> DatabaseMetadata {
            DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![TableSummary::new(
                    "public".to_string(),
                    "users".to_string(),
                    Some(100),
                    false,
                )],
                fetched_at: now,
            }
        }

        #[test]
        fn confirm_selection_in_normal_mode_clears_stale_table_detail() {
            let now = Instant::now();
            let mut state = create_test_state();
            state.cache.metadata = Some(users_metadata(now));
            state.cache.table_detail = Some(stale_table_detail());
            state.ui.input_mode = InputMode::Normal;
            state.ui.focused_pane = FocusedPane::Explorer;
            state.ui.set_explorer_selection(Some(0));

            let _ = reduce(&mut state, Action::ConfirmSelection, now);

            assert!(state.cache.table_detail.is_none());
        }

        #[test]
        fn confirm_selection_in_table_picker_mode_clears_stale_table_detail() {
            let now = Instant::now();
            let mut state = create_test_state();
            state.cache.metadata = Some(users_metadata(now));
            state.cache.table_detail = Some(stale_table_detail());
            state.ui.input_mode = InputMode::TablePicker;
            state.ui.picker_selected = 0;

            let _ = reduce(&mut state, Action::ConfirmSelection, now);

            assert!(state.cache.table_detail.is_none());
        }
    }

    mod effect_producing_actions {
        use super::*;
        use crate::domain::{DatabaseMetadata, MetadataState};

        #[test]
        fn load_metadata_with_dsn_returns_fetch_effect() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            let now = Instant::now();

            let effects = reduce(&mut state, Action::LoadMetadata, now);

            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::FetchMetadata { .. }));
            assert!(matches!(state.cache.state, MetadataState::Loading));
        }

        #[test]
        fn load_metadata_without_dsn_returns_no_effects() {
            let mut state = create_test_state();
            state.runtime.dsn = None;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::LoadMetadata, now);

            assert!(effects.is_empty());
        }

        #[test]
        fn reload_metadata_returns_sequence_effect() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ReloadMetadata, now);

            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::Sequence(_)));

            if let Effect::Sequence(seq) = &effects[0] {
                assert_eq!(seq.len(), 3);
                assert!(matches!(seq[0], Effect::CacheInvalidate { .. }));
                assert!(matches!(seq[1], Effect::ClearCompletionEngineCache));
                assert!(matches!(seq[2], Effect::FetchMetadata { .. }));
            }
        }

        #[test]
        fn reload_metadata_sets_is_reloading_flag() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            let now = Instant::now();

            let _ = reduce(&mut state, Action::ReloadMetadata, now);

            assert!(state.runtime.is_reloading);
        }

        #[test]
        fn reload_then_metadata_loaded_shows_reloaded_message() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            let now = Instant::now();

            // Trigger reload
            let _ = reduce(&mut state, Action::ReloadMetadata, now);
            assert!(state.runtime.is_reloading);

            // Metadata loaded
            let metadata = DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: now,
            };
            let _ = reduce(&mut state, Action::MetadataLoaded(Box::new(metadata)), now);

            // Check reloading flag is cleared and message is shown
            assert!(!state.runtime.is_reloading);
            assert_eq!(state.messages.last_success, Some("Reloaded!".to_string()));
        }

        #[test]
        fn execute_adhoc_with_dsn_returns_effect() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::ExecuteAdhoc("SELECT 1".to_string()),
                now,
            );

            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::ExecuteAdhoc { .. }));
        }
    }

    mod er_diagram {
        use super::*;
        use crate::app::er_state::ErStatus;
        use crate::domain::DatabaseMetadata;

        #[test]
        fn er_open_while_rendering_returns_no_effects() {
            let mut state = create_test_state();
            state.er_preparation.status = ErStatus::Rendering;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErOpenDiagram, now);

            assert!(effects.is_empty());
        }

        #[test]
        fn er_open_with_incomplete_prefetch_sets_waiting() {
            let mut state = create_test_state();
            state.cache.metadata = Some(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: Instant::now(),
            });
            state.sql_modal.prefetch_started = true;
            state
                .er_preparation
                .pending_tables
                .insert("public.users".to_string());
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErOpenDiagram, now);

            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
            assert!(effects.is_empty());
        }

        #[test]
        fn er_open_when_complete_returns_generate_effect() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.cache.metadata = Some(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: Instant::now(),
            });
            state.sql_modal.prefetch_started = true;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErOpenDiagram, now);

            assert_eq!(effects.len(), 1);
            assert!(matches!(
                effects[0],
                Effect::GenerateErDiagramFromCache { .. }
            ));
            assert_eq!(state.er_preparation.status, ErStatus::Rendering);
        }

        #[test]
        fn er_open_without_prefetch_starts_prefetch() {
            let mut state = create_test_state();
            state.cache.metadata = Some(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: Instant::now(),
            });
            // prefetch_started is false by default
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErOpenDiagram, now);

            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::DispatchActions(_)));
        }

        #[test]
        fn er_open_without_metadata_shows_error() {
            let mut state = create_test_state();
            // No metadata
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErOpenDiagram, now);

            assert!(state.messages.last_error.is_some());
            assert!(effects.is_empty());
        }
    }

    mod table_detail_cached {
        use super::*;
        use crate::domain::Table;

        fn make_test_table() -> Box<Table> {
            Box::new(Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                owner: None,
                columns: vec![],
                primary_key: None,
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            })
        }

        #[test]
        fn table_detail_cached_returns_cache_effect() {
            let mut state = create_test_state();
            state
                .sql_modal
                .prefetching_tables
                .insert("public.users".to_string());
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::TableDetailCached {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    detail: make_test_table(),
                },
                now,
            );

            assert!(!effects.is_empty());
            assert!(matches!(
                effects[0],
                Effect::CacheTableInCompletionEngine { .. }
            ));
            assert!(!state.sql_modal.prefetching_tables.contains("public.users"));
        }

        #[test]
        fn table_detail_cached_with_queue_returns_process_effect() {
            let mut state = create_test_state();
            state
                .sql_modal
                .prefetch_queue
                .push_back("public.orders".to_string());
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::TableDetailCached {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    detail: make_test_table(),
                },
                now,
            );

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ProcessPrefetchQueue))
            );
        }
    }

    mod connection_setup_validation {
        use crate::app::connection_setup_state::{ConnectionField, ConnectionSetupState};
        use crate::app::reducers::{validate_all, validate_field};
        use rstest::rstest;

        fn setup_state() -> ConnectionSetupState {
            ConnectionSetupState::default()
        }

        #[rstest]
        #[case(ConnectionField::Host, "", true)]
        #[case(ConnectionField::Host, "  ", true)]
        #[case(ConnectionField::Host, "localhost", false)]
        #[case(ConnectionField::Database, "", true)]
        #[case(ConnectionField::Database, "mydb", false)]
        #[case(ConnectionField::User, "", true)]
        #[case(ConnectionField::User, "postgres", false)]
        fn required_field_validation(
            #[case] field: ConnectionField,
            #[case] value: &str,
            #[case] has_error: bool,
        ) {
            let mut state = setup_state();
            match field {
                ConnectionField::Host => state.host = value.to_string(),
                ConnectionField::Database => state.database = value.to_string(),
                ConnectionField::User => state.user = value.to_string(),
                _ => {}
            }

            validate_field(&mut state, field);

            assert_eq!(state.validation_errors.contains_key(&field), has_error);
        }

        #[rstest]
        #[case("", true)]
        #[case("abc", true)]
        #[case("0", true)]
        #[case("1", false)]
        #[case("5432", false)]
        #[case("65535", false)]
        #[case("65536", true)]
        #[case("99999", true)]
        fn port_validation(#[case] value: &str, #[case] has_error: bool) {
            let mut state = setup_state();
            state.port = value.to_string();

            validate_field(&mut state, ConnectionField::Port);

            assert_eq!(
                state.validation_errors.contains_key(&ConnectionField::Port),
                has_error
            );
        }

        #[rstest]
        #[case(ConnectionField::Password)]
        #[case(ConnectionField::SslMode)]
        fn optional_fields_never_error(#[case] field: ConnectionField) {
            let mut state = setup_state();
            state.password = String::new();

            validate_field(&mut state, field);

            assert!(!state.validation_errors.contains_key(&field));
        }

        #[test]
        fn validate_all_checks_all_required_fields() {
            let mut state = setup_state();
            state.host = String::new();
            state.port = "invalid".to_string();
            state.database = String::new();
            state.user = String::new();

            validate_all(&mut state);

            assert!(state.validation_errors.contains_key(&ConnectionField::Host));
            assert!(state.validation_errors.contains_key(&ConnectionField::Port));
            assert!(
                state
                    .validation_errors
                    .contains_key(&ConnectionField::Database)
            );
            assert!(state.validation_errors.contains_key(&ConnectionField::User));
            assert!(
                !state
                    .validation_errors
                    .contains_key(&ConnectionField::Password)
            );
            assert!(
                !state
                    .validation_errors
                    .contains_key(&ConnectionField::SslMode)
            );
        }
    }

    mod connection_setup_transitions {
        use super::*;
        use crate::domain::ConnectionId;

        #[test]
        fn save_completed_sets_dsn_and_returns_fetch_effect() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConnectionSetup;
            state.connection_setup.is_first_run = true;
            state.connection_setup.host = "db.example.com".to_string();
            state.connection_setup.port = "5432".to_string();
            state.connection_setup.database = "mydb".to_string();
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::ConnectionSaveCompleted {
                    id: ConnectionId::new(),
                    dsn: "postgres://db.example.com/mydb".to_string(),
                    name: "Test Connection".to_string(),
                },
                now,
            );

            assert!(!state.connection_setup.is_first_run);
            assert_eq!(
                state.runtime.dsn,
                Some("postgres://db.example.com/mydb".to_string())
            );
            assert_eq!(
                state.runtime.active_connection_name,
                Some("Test Connection".to_string())
            );
            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::FetchMetadata { .. }));
        }

        #[test]
        fn save_failed_sets_error_message() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConnectionSetup;
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::ConnectionSaveFailed("Write error".to_string()),
                now,
            );

            assert!(state.messages.last_error.is_some());
            assert!(effects.is_empty());
        }

        #[test]
        fn cancel_on_first_run_opens_confirm_dialog() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConnectionSetup;
            state.connection_setup.is_first_run = true;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ConnectionSetupCancel, now);

            assert_eq!(state.ui.input_mode, InputMode::ConfirmDialog);
            assert!(matches!(state.confirm_dialog.on_confirm, Action::Quit));
            assert!(matches!(
                state.confirm_dialog.on_cancel,
                Action::OpenConnectionSetup
            ));
            assert!(effects.is_empty());
        }

        #[test]
        fn cancel_after_save_returns_to_normal_and_dispatches_try_connect() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConnectionSetup;
            state.connection_setup.is_first_run = false;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ConnectionSetupCancel, now);

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::DispatchActions(_)));
        }
    }

    mod confirm_dialog_transitions {
        use super::*;

        #[test]
        fn confirm_executes_on_confirm_action() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.confirm_dialog.on_confirm = Action::Quit;
            state.confirm_dialog.on_cancel = Action::OpenConnectionSetup;
            let now = Instant::now();

            let _ = reduce(&mut state, Action::ConfirmDialogConfirm, now);

            assert!(state.should_quit);
            assert!(matches!(state.confirm_dialog.on_confirm, Action::None));
            assert!(matches!(state.confirm_dialog.on_cancel, Action::None));
        }

        #[test]
        fn cancel_executes_on_cancel_action() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.confirm_dialog.on_confirm = Action::Quit;
            state.confirm_dialog.on_cancel = Action::OpenConnectionSetup;
            let now = Instant::now();

            let _ = reduce(&mut state, Action::ConfirmDialogCancel, now);

            assert_eq!(state.ui.input_mode, InputMode::ConnectionSetup);
            assert!(matches!(state.confirm_dialog.on_confirm, Action::None));
            assert!(matches!(state.confirm_dialog.on_cancel, Action::None));
        }
    }

    mod connection_state_tests {
        use super::*;
        use crate::app::connection_state::ConnectionState;
        use crate::domain::{ConnectionId, DatabaseMetadata, MetadataState};

        #[test]
        fn try_connect_with_dsn_starts_connecting() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.runtime.connection_state = ConnectionState::NotConnected;
            state.ui.input_mode = InputMode::Normal;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::TryConnect, now);

            assert!(state.runtime.connection_state.is_connecting());
            assert!(matches!(state.cache.state, MetadataState::Loading));
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::FetchMetadata { .. }));
        }

        #[test]
        fn try_connect_without_dsn_does_nothing() {
            let mut state = create_test_state();
            state.runtime.dsn = None;
            state.runtime.connection_state = ConnectionState::NotConnected;
            state.ui.input_mode = InputMode::Normal;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::TryConnect, now);

            assert!(state.runtime.connection_state.is_not_connected());
            assert!(effects.is_empty());
        }

        #[test]
        fn try_connect_when_already_connecting_is_noop() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.runtime.connection_state = ConnectionState::Connecting;
            state.ui.input_mode = InputMode::Normal;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::TryConnect, now);

            assert!(state.runtime.connection_state.is_connecting());
            assert!(effects.is_empty());
        }

        #[test]
        fn try_connect_when_already_connected_is_noop() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.runtime.connection_state = ConnectionState::Connected;
            state.ui.input_mode = InputMode::Normal;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::TryConnect, now);

            assert!(state.runtime.connection_state.is_connected());
            assert!(effects.is_empty());
        }

        #[test]
        fn try_connect_when_not_in_normal_mode_is_noop() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.runtime.connection_state = ConnectionState::NotConnected;
            state.ui.input_mode = InputMode::ConnectionSetup;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::TryConnect, now);

            assert!(state.runtime.connection_state.is_not_connected());
            assert!(effects.is_empty());
        }

        #[test]
        fn metadata_loaded_sets_connected() {
            let mut state = create_test_state();
            state.runtime.connection_state = ConnectionState::Connecting;
            let metadata = DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: Instant::now(),
            };
            let now = Instant::now();

            let _ = reduce(&mut state, Action::MetadataLoaded(Box::new(metadata)), now);

            assert!(state.runtime.connection_state.is_connected());
            assert!(matches!(state.cache.state, MetadataState::Loaded));
        }

        #[test]
        fn metadata_failed_sets_failed() {
            let mut state = create_test_state();
            state.runtime.connection_state = ConnectionState::Connecting;
            let now = Instant::now();

            let _ = reduce(
                &mut state,
                Action::MetadataFailed("connection refused".to_string()),
                now,
            );

            assert!(state.runtime.connection_state.is_failed());
            assert!(matches!(state.cache.state, MetadataState::Error(_)));
        }

        #[test]
        fn metadata_failed_preserves_connected_state() {
            // When already connected, metadata failure should preserve connection state
            // (metadata-only failure, e.g., permission denied on schema)
            let mut state = create_test_state();
            state.runtime.connection_state = ConnectionState::Connected;
            state.cache.state = MetadataState::Loaded;
            let now = Instant::now();

            let _ = reduce(
                &mut state,
                Action::MetadataFailed("permission denied".to_string()),
                now,
            );

            // Connection state should remain Connected
            assert!(state.runtime.connection_state.is_connected());
            // But metadata state should be Error
            assert!(matches!(state.cache.state, MetadataState::Error(_)));
        }

        #[test]
        fn reenter_connection_setup_resets_all_states() {
            let mut state = create_test_state();
            state.runtime.connection_state = ConnectionState::Failed;
            state.cache.state = MetadataState::Error("error".to_string());
            state.ui.input_mode = InputMode::ConnectionError;
            let now = Instant::now();

            let _ = reduce(&mut state, Action::ReenterConnectionSetup, now);

            assert!(state.runtime.connection_state.is_not_connected());
            assert!(matches!(state.cache.state, MetadataState::NotLoaded));
            assert_eq!(state.ui.input_mode, InputMode::ConnectionSetup);
        }

        #[test]
        fn reenter_connection_setup_preserves_form_values() {
            let mut state = create_test_state();
            state.connection_setup.host = "custom-host".to_string();
            state.connection_setup.port = "5433".to_string();
            state.connection_setup.database = "mydb".to_string();
            state.connection_setup.user = "admin".to_string();
            state.connection_setup.password = "secret".to_string();
            state.runtime.connection_state = ConnectionState::Failed;
            let now = Instant::now();

            let _ = reduce(&mut state, Action::ReenterConnectionSetup, now);

            assert_eq!(state.connection_setup.host, "custom-host");
            assert_eq!(state.connection_setup.port, "5433");
            assert_eq!(state.connection_setup.database, "mydb");
            assert_eq!(state.connection_setup.user, "admin");
            assert_eq!(state.connection_setup.password, "secret");
        }

        #[test]
        fn connection_save_completed_sets_connecting_and_loading() {
            let mut state = create_test_state();
            state.runtime.connection_state = ConnectionState::NotConnected;
            state.cache.state = MetadataState::NotLoaded;
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::ConnectionSaveCompleted {
                    id: ConnectionId::new(),
                    dsn: "postgres://localhost/test".to_string(),
                    name: "Test".to_string(),
                },
                now,
            );

            assert!(state.runtime.connection_state.is_connecting());
            assert!(matches!(state.cache.state, MetadataState::Loading));
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::FetchMetadata { .. }));
        }

        #[test]
        fn switch_connection_saves_current_and_fetches_new() {
            let mut state = create_test_state();
            let conn_a = ConnectionId::new();
            let conn_b = ConnectionId::new();

            state.runtime.active_connection_id = Some(conn_a.clone());
            state.runtime.connection_state = ConnectionState::Connected;
            state.ui.explorer_selected = 5;
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::SwitchConnection {
                    id: conn_b.clone(),
                    dsn: "postgres://localhost/other".to_string(),
                    name: "Other".to_string(),
                },
                now,
            );

            assert_eq!(state.runtime.active_connection_id, Some(conn_b));
            assert!(state.runtime.connection_state.is_connecting());
            assert!(state.connection_caches.get(&conn_a).is_some());
            assert_eq!(
                state
                    .connection_caches
                    .get(&conn_a)
                    .unwrap()
                    .explorer_selected,
                5
            );
            assert_eq!(effects.len(), 2);
        }

        #[test]
        fn switch_connection_restores_from_cache() {
            use crate::app::inspector_tab::InspectorTab;

            let mut state = create_test_state();
            let conn_a = ConnectionId::new();
            let conn_b = ConnectionId::new();

            state.runtime.active_connection_id = Some(conn_a.clone());
            state.runtime.connection_state = ConnectionState::Connected;
            state.ui.explorer_selected = 3;

            let cached = crate::app::connection_cache::ConnectionCache {
                explorer_selected: 10,
                inspector_tab: InspectorTab::Indexes,
                metadata: Some(DatabaseMetadata {
                    database_name: "cached_db".to_string(),
                    schemas: vec![],
                    tables: vec![],
                    fetched_at: Instant::now(),
                }),
                ..Default::default()
            };
            state.connection_caches.save(&conn_b, cached);
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::SwitchConnection {
                    id: conn_b.clone(),
                    dsn: "postgres://localhost/cached".to_string(),
                    name: "Cached".to_string(),
                },
                now,
            );

            assert_eq!(state.runtime.active_connection_id, Some(conn_b));
            assert!(state.runtime.connection_state.is_connected());
            assert_eq!(state.ui.explorer_selected, 10);
            assert_eq!(state.ui.inspector_tab, InspectorTab::Indexes);
            assert_eq!(
                state.cache.metadata.as_ref().unwrap().database_name,
                "cached_db"
            );
            assert_eq!(effects.len(), 1);
        }
    }

    mod er_table_picker {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn state_with_metadata() -> AppState {
            let mut state = create_test_state();
            state.cache.metadata = Some(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![
                    TableSummary::new("public".to_string(), "users".to_string(), None, false),
                    TableSummary::new("public".to_string(), "posts".to_string(), None, false),
                ],
                fetched_at: Instant::now(),
            });
            state
        }

        #[test]
        fn open_clears_selections_and_filter() {
            let mut state = state_with_metadata();
            state.ui.er_filter_input = "old".to_string();
            state
                .ui
                .er_selected_tables
                .insert("public.users".to_string());
            let now = Instant::now();

            let effects = reduce(&mut state, Action::OpenErTablePicker, now);

            assert_eq!(state.ui.input_mode, InputMode::ErTablePicker);
            assert!(state.ui.er_filter_input.is_empty());
            assert!(state.ui.er_selected_tables.is_empty());
            assert!(effects.is_empty());
        }

        #[test]
        fn open_without_metadata_sets_pending() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::OpenErTablePicker, now);

            assert!(state.ui.pending_er_picker);
            assert!(state.messages.last_success.is_some());
            assert_ne!(state.ui.input_mode, InputMode::ErTablePicker);
            assert!(effects.is_empty());
        }

        fn sample_metadata() -> Box<DatabaseMetadata> {
            Box::new(DatabaseMetadata {
                database_name: "test_db".to_string(),
                schemas: vec![],
                tables: vec![TableSummary::new(
                    "public".to_string(),
                    "users".to_string(),
                    Some(100),
                    false,
                )],
                fetched_at: Instant::now(),
            })
        }

        fn has_open_er_dispatch(effects: &[Effect]) -> bool {
            effects.iter().any(|e| {
                matches!(e, Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::OpenErTablePicker)))
            })
        }

        #[test]
        fn metadata_loaded_with_pending_dispatches_open() {
            let mut state = create_test_state();
            state.ui.pending_er_picker = true;
            state.ui.input_mode = InputMode::Normal;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::MetadataLoaded(sample_metadata()), now);

            assert!(!state.ui.pending_er_picker);
            assert!(has_open_er_dispatch(&effects));
        }

        #[test]
        fn metadata_loaded_without_pending_does_not_dispatch_open() {
            let mut state = create_test_state();
            state.ui.pending_er_picker = false;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::MetadataLoaded(sample_metadata()), now);

            assert!(!has_open_er_dispatch(&effects));
        }

        #[test]
        fn metadata_loaded_with_pending_but_non_normal_mode_discards() {
            let mut state = create_test_state();
            state.ui.pending_er_picker = true;
            state.ui.input_mode = InputMode::SqlModal;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::MetadataLoaded(sample_metadata()), now);

            assert!(!state.ui.pending_er_picker);
            assert!(!has_open_er_dispatch(&effects));
        }

        #[test]
        fn close_er_table_picker_returns_to_normal() {
            let mut state = state_with_metadata();
            state.ui.input_mode = InputMode::ErTablePicker;
            state.ui.er_filter_input = "test".to_string();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::CloseErTablePicker, now);

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert!(state.ui.er_filter_input.is_empty());
            assert!(effects.is_empty());
        }

        #[test]
        fn confirm_with_selected_tables_sets_target_and_returns_dispatch() {
            let mut state = state_with_metadata();
            state.ui.input_mode = InputMode::ErTablePicker;
            state
                .ui
                .er_selected_tables
                .insert("public.users".to_string());
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErConfirmSelection, now);

            assert_eq!(
                state.er_preparation.target_tables,
                vec!["public.users".to_string()]
            );
            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::DispatchActions(_)));
        }

        #[test]
        fn confirm_with_no_selection_returns_error() {
            let mut state = state_with_metadata();
            state.ui.input_mode = InputMode::ErTablePicker;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErConfirmSelection, now);

            assert_eq!(state.ui.input_mode, InputMode::ErTablePicker);
            assert!(state.messages.last_error.is_some());
            assert!(effects.is_empty());
        }

        #[test]
        fn er_open_with_target_tables_returns_generate_effect() {
            let mut state = state_with_metadata();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.sql_modal.prefetch_started = true;
            state.er_preparation.target_tables = vec!["public.users".to_string()];
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErOpenDiagram, now);

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::GenerateErDiagramFromCache { target_tables, .. } => {
                    assert_eq!(target_tables, &vec!["public.users".to_string()]);
                }
                other => panic!("expected GenerateErDiagramFromCache, got {:?}", other),
            }
        }

        #[test]
        fn prefetch_complete_auto_dispatches_er_open() {
            use crate::app::er_state::ErStatus;

            let mut state = state_with_metadata();
            state.sql_modal.prefetch_started = true;
            state.er_preparation.status = ErStatus::Waiting;
            state.er_preparation.total_tables = 1;
            state
                .er_preparation
                .pending_tables
                .insert("public.users".to_string());
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::TableDetailAlreadyCached {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                now,
            );

            assert_eq!(state.er_preparation.status, ErStatus::Idle);
            assert!(effects.iter().any(|e| {
                matches!(e, Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::ErOpenDiagram)))
            }));
        }

        #[test]
        fn prefetch_complete_with_failures_does_not_auto_open() {
            use crate::app::er_state::ErStatus;

            let mut state = state_with_metadata();
            state.sql_modal.prefetch_started = true;
            state.er_preparation.status = ErStatus::Waiting;
            state.er_preparation.total_tables = 2;
            state
                .er_preparation
                .failed_tables
                .insert("public.posts".to_string(), "timeout".to_string());
            state
                .er_preparation
                .pending_tables
                .insert("public.users".to_string());
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::TableDetailAlreadyCached {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                now,
            );

            assert_eq!(state.er_preparation.status, ErStatus::Idle);
            assert!(!effects.iter().any(|e| {
                matches!(e, Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::ErOpenDiagram)))
            }));
            assert!(state.messages.last_error.is_some());
        }
    }

    mod pagination_integration {
        use super::*;
        use crate::app::query_execution::PREVIEW_PAGE_SIZE;
        use crate::domain::{DatabaseMetadata, QueryResult, QuerySource, TableSummary};
        use std::sync::Arc;

        /// Set up a state with metadata loaded, a table selected via
        /// ConfirmSelection, and a preview result completed.
        fn state_after_confirm_and_complete() -> (AppState, Instant) {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            let now = Instant::now();

            // Load metadata with a table
            let metadata = DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![TableSummary::new(
                    "public".to_string(),
                    "users".to_string(),
                    Some(1200),
                    false,
                )],
                fetched_at: now,
            };
            reduce(&mut state, Action::MetadataLoaded(Box::new(metadata)), now);

            // ConfirmSelection from Normal mode (explorer focused)
            state.ui.input_mode = InputMode::Normal;
            state.ui.focused_pane = FocusedPane::Explorer;
            state.ui.explorer_selected = 0;
            let effects = reduce(&mut state, Action::ConfirmSelection, now);

            // Extract the dispatched ExecutePreview action and run it
            let dispatch_actions: Vec<Action> = effects
                .into_iter()
                .filter_map(|e| match e {
                    Effect::DispatchActions(actions) => Some(actions),
                    _ => None,
                })
                .flatten()
                .collect();
            for action in dispatch_actions {
                reduce(&mut state, action, now);
            }

            // Simulate QueryCompleted with a full page of results
            let current_gen = state.cache.selection_generation;
            let result = Arc::new(QueryResult {
                columns: vec!["id".to_string()],
                rows: vec![vec!["1".to_string()]; PREVIEW_PAGE_SIZE],
                execution_time_ms: 10,
                source: QuerySource::Preview,
                row_count: PREVIEW_PAGE_SIZE,
                query: String::new(),
                executed_at: now,
                error: None,
            });
            reduce(
                &mut state,
                Action::QueryCompleted {
                    result,
                    generation: current_gen,
                    target_page: Some(0),
                },
                now,
            );

            (state, now)
        }

        #[test]
        fn confirm_selection_initializes_pagination_via_dispatch() {
            let (state, _now) = state_after_confirm_and_complete();

            assert_eq!(state.query.pagination.schema, "public");
            assert_eq!(state.query.pagination.table, "users");
            assert_eq!(state.query.pagination.total_rows_estimate, Some(1200));
            assert_eq!(state.query.pagination.current_page, 0);
            assert!(!state.query.pagination.reached_end);
        }

        #[test]
        fn next_page_after_confirm_emits_correct_offset() {
            let (mut state, now) = state_after_confirm_and_complete();

            let effects = reduce(&mut state, Action::ResultNextPage, now);

            let preview_effect = effects
                .iter()
                .find(|e| matches!(e, Effect::ExecutePreview { .. }));
            assert!(preview_effect.is_some());
            if let Some(Effect::ExecutePreview {
                offset,
                target_page,
                schema,
                table,
                ..
            }) = preview_effect
            {
                assert_eq!(*offset, PREVIEW_PAGE_SIZE);
                assert_eq!(*target_page, 1);
                assert_eq!(schema, "public");
                assert_eq!(table, "users");
            }
        }
    }

    mod command_palette {
        use super::*;
        use crate::app::palette::palette_commands;
        use rstest::rstest;

        fn state_in_palette_mode() -> AppState {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::CommandPalette;
            state
        }

        fn palette_index_of(target: impl Fn(&Action) -> bool) -> usize {
            palette_commands()
                .enumerate()
                .find(|(_, kb)| target(&kb.action))
                .map(|(i, _)| i)
                .expect("action must exist in palette")
        }

        #[rstest]
        #[case(Action::OpenHelp, InputMode::Help)]
        #[case(Action::OpenTablePicker, InputMode::TablePicker)]
        #[case(Action::OpenSqlModal, InputMode::SqlModal)]
        fn confirm_selection_applies_sub_action(
            #[case] target_action: Action,
            #[case] expected_mode: InputMode,
        ) {
            let entry_index = palette_index_of(|a| {
                std::mem::discriminant(a) == std::mem::discriminant(&target_action)
            });

            let mut state = state_in_palette_mode();
            state.ui.picker_selected = entry_index;
            let now = Instant::now();

            let _ = reduce(&mut state, Action::ConfirmSelection, now);

            assert_eq!(state.ui.input_mode, expected_mode);
        }

        #[test]
        fn confirm_selection_with_reload_emits_sequence_effect() {
            let entry_index = palette_index_of(|a| matches!(a, Action::ReloadMetadata));

            let mut state = state_in_palette_mode();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.ui.picker_selected = entry_index;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ConfirmSelection, now);

            assert!(
                effects.iter().any(|e| matches!(e, Effect::Sequence(_))),
                "expected Sequence effect for ReloadMetadata, got {:?}",
                effects
            );
        }

        #[test]
        fn confirm_selection_toggle_explorer_mode_closes_palette() {
            let entry_index = palette_index_of(|a| matches!(a, Action::ToggleExplorerMode));

            let mut state = state_in_palette_mode();
            state.ui.picker_selected = entry_index;
            let now = Instant::now();

            let _ = reduce(&mut state, Action::ConfirmSelection, now);

            assert_ne!(
                state.ui.input_mode,
                InputMode::CommandPalette,
                "palette must be closed after confirm"
            );
        }
    }
}
