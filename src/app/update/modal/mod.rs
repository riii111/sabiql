pub(in crate::update) mod base;
pub(in crate::update) mod confirm_dialog;
pub(in crate::update) mod er_picker;
pub(in crate::update) mod help;
pub(in crate::update) mod query_history;
pub(in crate::update) mod settings;
pub(in crate::update) mod sqlite_diagnostics;

#[cfg(test)]
mod tests {
    use crate::cmd::effect::Effect;
    use crate::domain::{ConnectionId, DatabaseType, QueryValue};
    use crate::model::app_state::AppState;
    use crate::model::browse::query_execution::PostDeleteRowSelection;
    use crate::model::shared::confirm_dialog::{ConfirmIntent, CsvExportCacheSnapshot};
    use crate::model::shared::help::HelpMode;
    use crate::model::shared::input_mode::InputMode;
    use crate::model::shared::settings::{IconMode, KeymapPreset};
    use crate::ports::outbound::AppSettings;
    use crate::services::AppServices;
    use crate::update::action::{
        Action, CursorMove, InputTarget, ListMotion, ListTarget, ModalKind, ScrollAmount,
        ScrollDirection, ScrollTarget, TextKillDirection,
    };
    use crate::update::reducer::{reduce, route};
    use crate::update::test_fixtures;
    use crate::update::test_fixtures::edit_text;
    use std::time::Instant;

    fn create_test_state() -> AppState {
        AppState::new("test".to_string())
    }

    mod base {
        use super::*;

        #[test]
        fn escape_closes_connection_selector() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::ConnectionSelector);

            let effects = route(
                &mut state,
                Action::Escape,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(effects.is_empty());
        }

        #[test]
        fn escape_keeps_modal_with_specific_close_action_open() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::SqlModal);

            route(
                &mut state,
                Action::Escape,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.input_mode(), InputMode::SqlModal);
        }
    }

    mod help {
        use super::*;

        fn open_help(state: &mut AppState) {
            route(
                state,
                Action::OpenModal(ModalKind::CommandPalette),
                Instant::now(),
                &AppServices::stub(),
            );
            route(
                state,
                Action::ToggleHelp,
                Instant::now(),
                &AppServices::stub(),
            );
            assert_eq!(state.input_mode(), InputMode::Help);
        }

        #[test]
        fn escape_returns_to_help_origin_with_filter() {
            let mut state = create_test_state();
            open_help(&mut state);
            state.ui.help_mut().insert_filter_char('c');

            let effects = route(
                &mut state,
                Action::CloseModal(ModalKind::Help),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.input_mode(), InputMode::CommandPalette);
            assert!(state.ui.help().filter().content().is_empty());
            assert!(effects.is_empty());
        }

        #[test]
        fn escape_returns_to_help_origin_when_filter_is_empty() {
            let mut state = create_test_state();
            open_help(&mut state);

            let effects = route(
                &mut state,
                Action::CloseModal(ModalKind::Help),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.input_mode(), InputMode::CommandPalette);
            assert!(effects.is_empty());
        }

        #[test]
        fn filter_text_actions_update_help_state() {
            let mut state = create_test_state();
            open_help(&mut state);

            let input_effects = edit_text(
                &mut state,
                &Action::TextInput {
                    target: InputTarget::HelpFilter,
                    ch: 'k',
                },
                Instant::now(),
            );
            let backspace_effects = edit_text(
                &mut state,
                &Action::TextBackspace {
                    target: InputTarget::HelpFilter,
                },
                Instant::now(),
            );

            assert!(state.ui.help().filter().content().is_empty());
            assert!(input_effects.is_empty());
            assert!(backspace_effects.is_empty());
        }

        #[test]
        fn focused_help_filter_applies_readline_edits() {
            let mut state = create_test_state();
            open_help(&mut state);

            route(
                &mut state,
                Action::EnterHelpFilter,
                Instant::now(),
                &AppServices::stub(),
            );
            edit_text(
                &mut state,
                &Action::TextInput {
                    target: InputTarget::HelpFilter,
                    ch: 'x',
                },
                Instant::now(),
            );
            edit_text(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::HelpFilter,
                    direction: CursorMove::LineStart,
                },
                Instant::now(),
            );
            edit_text(
                &mut state,
                &Action::TextKill {
                    target: InputTarget::HelpFilter,
                    direction: TextKillDirection::ToLineEnd,
                },
                Instant::now(),
            );
            edit_text(
                &mut state,
                &Action::TextYank {
                    target: InputTarget::HelpFilter,
                },
                Instant::now(),
            );

            assert_eq!(state.ui.help().mode(), HelpMode::EditingFilter);
            assert_eq!(state.ui.help().filter().content(), "x");
            assert_eq!(state.ui.help().filter().cursor(), 1);
        }

        #[test]
        fn exiting_help_filter_keeps_the_help_modal_open() {
            let mut state = create_test_state();
            open_help(&mut state);
            route(
                &mut state,
                Action::EnterHelpFilter,
                Instant::now(),
                &AppServices::stub(),
            );

            route(
                &mut state,
                Action::ExitHelpFilter,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.input_mode(), InputMode::Help);
            assert_eq!(state.ui.help().mode(), HelpMode::Viewing);
        }
    }

    mod readline_edits {
        use super::*;

        fn kill_then_yank(state: &mut AppState, target: InputTarget) {
            edit_text(
                state,
                &Action::TextKill {
                    target,
                    direction: TextKillDirection::ToLineStart,
                },
                Instant::now(),
            );
            edit_text(state, &Action::TextYank { target }, Instant::now());
        }

        #[test]
        fn er_filter_kill_then_yank_restores_text() {
            let mut state = create_test_state();
            state.ui.er_picker_mut().insert_filter_str("users");

            kill_then_yank(&mut state, InputTarget::ErFilter);

            assert_eq!(state.ui.er_picker().filter_input().content(), "users");
            assert_eq!(state.kill_buffer(), Some("users"));
        }

        #[test]
        fn query_history_filter_kill_then_yank_restores_text() {
            let mut state = create_test_state();
            state.query_history_picker.insert_filter_str("SELECT");

            kill_then_yank(&mut state, InputTarget::QueryHistoryFilter);

            assert_eq!(
                state.query_history_picker.filter_input().content(),
                "SELECT"
            );
            assert_eq!(state.kill_buffer(), Some("SELECT"));
        }

        #[test]
        fn settings_browser_kill_then_yank_restores_text() {
            let mut state = create_test_state();
            state.settings.switch_next_section();
            state.settings.switch_next_section();
            state.settings.start_custom_browser_edit();
            for ch in "Firefox".chars() {
                state.settings.input_custom_browser(ch);
            }

            kill_then_yank(&mut state, InputTarget::SettingsErBrowser);

            assert_eq!(state.settings.custom_er_browser().content(), "Firefox");
            assert_eq!(state.kill_buffer(), Some("Firefox"));
        }
    }

    mod settings {
        use super::*;
        use crate::model::shared::theme_id::ThemeId;

        mod theme_selection {
            use super::*;
            use rstest::rstest;

            #[test]
            fn opens_with_current_theme() {
                let mut state = create_test_state();
                state.ui.set_theme(ThemeId::Light);

                let effects = route(
                    &mut state,
                    Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::Settings);
                assert_eq!(state.settings.previous_theme(), ThemeId::Light);
                assert_eq!(state.settings.selected_theme(), ThemeId::Light);
                assert!(effects.is_empty());
            }

            #[test]
            fn navigates_without_applying() {
                let mut state = create_test_state();
                route(
                    &mut state,
                    Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                    &AppServices::stub(),
                );

                route(
                    &mut state,
                    Action::SettingsSelectNext,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.settings.selected_theme(), ThemeId::Light);
                assert_eq!(state.ui.theme_id(), ThemeId::Default);
            }

            #[rstest]
            #[case(Action::SettingsCancel)]
            #[case(Action::CloseModal(ModalKind::Settings))]
            fn cancel_discards_selection(#[case] action: Action) {
                let mut state = create_test_state();
                route(
                    &mut state,
                    Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                    &AppServices::stub(),
                );
                route(
                    &mut state,
                    Action::SettingsSelectNext,
                    Instant::now(),
                    &AppServices::stub(),
                );

                let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

                assert_eq!(state.input_mode(), InputMode::Normal);
                assert_eq!(state.settings.selected_theme(), ThemeId::Default);
                assert_eq!(state.ui.theme_id(), ThemeId::Default);
                assert!(effects.is_empty());
            }

            #[test]
            fn apply_emits_settings_save_effect() {
                let mut state = create_test_state();
                route(
                    &mut state,
                    Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                    &AppServices::stub(),
                );
                route(
                    &mut state,
                    Action::SettingsSelectNext,
                    Instant::now(),
                    &AppServices::stub(),
                );

                route(
                    &mut state,
                    Action::SettingsPreviousSection,
                    Instant::now(),
                    &AppServices::stub(),
                );
                route(
                    &mut state,
                    Action::SettingsSelectNext,
                    Instant::now(),
                    &AppServices::stub(),
                );

                let effects = route(
                    &mut state,
                    Action::SettingsApply,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::Settings);
                assert_eq!(state.ui.theme_id(), ThemeId::Default);
                assert_eq!(state.settings.saved_icon_mode(), IconMode::Unicode);
                assert!(matches!(
                    effects.as_slice(),
                    [Effect::SaveSettings { settings }]
                        if settings.theme_id == ThemeId::Light && settings.icon_mode == IconMode::Ascii && settings.er_browser.is_none()
                ));
            }

            #[test]
            fn apply_emits_er_browser_save_effect() {
                let mut state = create_test_state();
                route(
                    &mut state,
                    Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                    &AppServices::stub(),
                );
                route(
                    &mut state,
                    Action::SettingsNextSection,
                    Instant::now(),
                    &AppServices::stub(),
                );
                route(
                    &mut state,
                    Action::SettingsNextSection,
                    Instant::now(),
                    &AppServices::stub(),
                );
                route(
                    &mut state,
                    Action::SettingsSelectNext,
                    Instant::now(),
                    &AppServices::stub(),
                );

                let effects = route(
                    &mut state,
                    Action::SettingsApply,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::Settings);
                assert_eq!(state.settings.saved_er_browser(), None);
                assert!(matches!(
                    effects.as_slice(),
                    [Effect::SaveSettings { settings }]
                        if settings.er_browser.as_deref() == Some("Google Chrome")
                ));
            }

            #[test]
            fn custom_browser_input_is_saved() {
                let mut state = create_test_state();
                route(
                    &mut state,
                    Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                    &AppServices::stub(),
                );
                route(
                    &mut state,
                    Action::SettingsNextSection,
                    Instant::now(),
                    &AppServices::stub(),
                );
                route(
                    &mut state,
                    Action::SettingsNextSection,
                    Instant::now(),
                    &AppServices::stub(),
                );
                route(
                    &mut state,
                    Action::SettingsStartCustomBrowserEdit,
                    Instant::now(),
                    &AppServices::stub(),
                );
                edit_text(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::SettingsErBrowser,
                        ch: 'B',
                    },
                    Instant::now(),
                );

                let effects = route(
                    &mut state,
                    Action::SettingsApply,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::Settings);
                assert!(matches!(
                    effects.as_slice(),
                    [Effect::SaveSettings { settings }]
                        if settings.er_browser.as_deref() == Some("B")
                ));
            }

            #[test]
            fn save_success_commits_pending_settings() {
                let mut state = create_test_state();
                route(
                    &mut state,
                    Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                    &AppServices::stub(),
                );
                route(
                    &mut state,
                    Action::SettingsSelectNext,
                    Instant::now(),
                    &AppServices::stub(),
                );
                route(
                    &mut state,
                    Action::SettingsNextSection,
                    Instant::now(),
                    &AppServices::stub(),
                );
                route(
                    &mut state,
                    Action::SettingsSelectNext,
                    Instant::now(),
                    &AppServices::stub(),
                );

                let effects = route(
                    &mut state,
                    Action::SettingsSaved(AppSettings {
                        theme_id: ThemeId::Light,
                        icon_mode: IconMode::Unicode,
                        keymap_preset: KeymapPreset::Ide,
                        er_browser: Some("Google Chrome".to_string()),
                    }),
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.ui.theme_id(), ThemeId::Light);
                assert_eq!(state.settings.previous_theme(), ThemeId::Light);
                assert_eq!(state.settings.saved_icon_mode(), IconMode::Unicode);
                assert_eq!(state.settings.saved_keymap_preset(), KeymapPreset::Ide);
                assert_eq!(state.settings.saved_er_browser(), Some("Google Chrome"));
                assert!(
                    state
                        .messages
                        .last_success
                        .as_deref()
                        .is_some_and(|message| { message.contains("Settings saved") })
                );
                assert!(effects.is_empty());
            }
        }

        #[test]
        fn save_failed_sets_error_message() {
            let mut state = create_test_state();

            let effects = route(
                &mut state,
                Action::SettingsSaveFailed("I/O error: disk full".to_string()),
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.messages.last_error.as_deref().is_some_and(|message| {
                message.contains("Failed to save settings") && message.contains("disk full")
            }));
            assert!(effects.is_empty());
        }
    }

    mod confirm_dialog {
        use super::*;

        pub(super) fn enter_confirm_dialog(state: &mut AppState, return_mode: InputMode) {
            state.modal.set_mode(return_mode);
            state.modal.push_mode(InputMode::ConfirmDialog);
        }

        const CSV_TEST_DSN: &str = "postgres://localhost/test";
        const CSV_TEST_FILE: &str = "test.csv";

        fn csv_state_with_current_run() -> (AppState, u64) {
            let mut state = create_test_state();
            enter_confirm_dialog(&mut state, InputMode::Normal);
            test_fixtures::activate_postgres_connection(&mut state, CSV_TEST_DSN);
            let run_id = state.query.begin_running(Instant::now());
            (state, run_id)
        }

        fn csv_state_with_stale_run() -> (AppState, u64, u64) {
            let (mut state, stale_run_id) = csv_state_with_current_run();
            let current_run_id = state.query.begin_running(Instant::now());
            (state, stale_run_id, current_run_id)
        }

        fn rerunnable_csv_intent(run_id: u64) -> ConfirmIntent {
            ConfirmIntent::CsvExportRerunnable {
                dsn: CSV_TEST_DSN.to_string(),
                run_id,
                export_query: "SELECT 1".to_string(),
                file_name: CSV_TEST_FILE.to_string(),
            }
        }

        fn cached_csv_intent(run_id: u64) -> ConfirmIntent {
            ConfirmIntent::CsvExportCached {
                dsn: CSV_TEST_DSN.to_string(),
                run_id,
                file_name: CSV_TEST_FILE.to_string(),
                row_count: Some(2),
                snapshot: CsvExportCacheSnapshot {
                    columns: vec!["id".to_string()],
                    values: vec![vec![QueryValue::text("1")]],
                },
            }
        }

        fn open_confirm_intent(state: &mut AppState, intent: ConfirmIntent) {
            state.confirm_dialog.open("", "", intent);
        }

        fn confirm_effects(state: &mut AppState) -> Vec<Effect> {
            route(
                state,
                Action::ConfirmDialogConfirm,
                Instant::now(),
                &AppServices::stub(),
            )
        }

        fn cancel_effects(state: &mut AppState) -> Vec<Effect> {
            route(
                state,
                Action::ConfirmDialogCancel,
                Instant::now(),
                &AppServices::stub(),
            )
        }

        fn assert_cached_export_effect(effect: &Effect, run_id: u64) {
            let Effect::ExportCsvFromCache {
                dsn,
                run_id: effect_run_id,
                file_name,
                columns,
                values,
                row_count,
            } = effect
            else {
                panic!("expected cached CSV export effect");
            };

            assert_eq!(dsn, CSV_TEST_DSN);
            assert_eq!(*effect_run_id, run_id);
            assert_eq!(file_name, CSV_TEST_FILE);
            assert_eq!(columns, &vec!["id".to_string()]);
            assert_eq!(values, &vec![vec![QueryValue::text("1")]]);
            assert_eq!(*row_count, Some(2));
        }

        mod confirm {
            use super::*;
            use crate::policy::write::write_guardrails::{
                GuardrailDecision, RiskLevel, TargetSummary, WriteOperation, WritePreview,
            };

            #[test]
            fn quit_no_connection_sets_should_quit() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state
                    .confirm_dialog
                    .open("", "", ConfirmIntent::QuitNoConnection);

                let effects = route(
                    &mut state,
                    Action::ConfirmDialogConfirm,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert!(state.should_quit);
                assert!(state.confirm_dialog.intent().is_none());
                assert!(matches!(effects.as_slice(), [Effect::CancelTrackedTasks]));
            }

            #[test]
            fn delete_connection_confirmation_dispatches_delete_action() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::ConnectionSelector);
                let id = ConnectionId::new();
                let _ = state.session.begin_mysql_connection_probe(
                    &id,
                    "MySQL",
                    "mysql://localhost/test",
                    Some("test"),
                );
                state
                    .confirm_dialog
                    .open("", "", ConfirmIntent::DeleteConnection(id.clone()));

                let effects = route(
                    &mut state,
                    Action::ConfirmDialogConfirm,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::ConnectionSelector);
                let [Effect::DispatchActions(actions)] = effects.as_slice() else {
                    panic!("expected DeleteConnection action dispatch, got {effects:?}");
                };
                assert!(matches!(
                    actions.as_slice(),
                    [Action::DeleteConnection(action_id)] if action_id == &id
                ));

                let effects = reduce(
                    &mut state,
                    actions[0].clone(),
                    Instant::now(),
                    &AppServices::stub(),
                );
                assert!(state.session.pending_mysql_connection_probe().is_none());
                assert!(matches!(
                    effects.as_slice(),
                    [Effect::DeleteConnection { id: effect_id }] if effect_id == &id
                ));
            }

            #[test]
            fn delete_success_then_write_then_preview_completion_clears_selection() {
                let mut state = test_fixtures::state_after_delete_success();
                enter_confirm_dialog(&mut state, InputMode::CellEdit);
                state.confirm_dialog.open(
                    "",
                    "",
                    ConfirmIntent::ExecuteWrite {
                        sql: "UPDATE t SET x=1".to_string(),
                        blocked: false,
                    },
                );

                let now = Instant::now();
                let effects = reduce(
                    &mut state,
                    Action::ConfirmDialogConfirm,
                    now,
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::CellEdit);
                assert!(state.query.is_running());
                assert!(state.query.start_time().is_some());
                assert_eq!(
                    state.query.post_delete_row_selection(),
                    PostDeleteRowSelection::Keep
                );
                assert_eq!(effects.len(), 1);
                assert!(matches!(&effects[0], Effect::ExecuteWrite { .. }));
                test_fixtures::complete_table_preview(&mut state, now);
                assert!(state.result_interaction.selection().row().is_none());
                assert!(state.result_interaction.selection().cell().is_none());
            }

            #[test]
            fn execute_write_no_dsn_sets_error() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state.session.clear_connection();
                state.confirm_dialog.open(
                    "",
                    "",
                    ConfirmIntent::ExecuteWrite {
                        sql: "UPDATE t SET x=1".to_string(),
                        blocked: false,
                    },
                );

                let effects = route(
                    &mut state,
                    Action::ConfirmDialogConfirm,
                    Instant::now(),
                    &AppServices::stub(),
                );

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

                let effects = route(
                    &mut state,
                    Action::ConfirmDialogConfirm,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::Normal);
                assert!(effects.is_empty());
            }

            #[test]
            fn execute_write_blocked_confirm_clears_preview_state() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state.result_interaction.set_write_preview(WritePreview {
                    operation: WriteOperation::Update,
                    sql: "UPDATE t SET x=1".to_string(),
                    target_summary: TargetSummary {
                        schema: "public".to_string(),
                        table: "t".to_string(),
                        key_values: vec![],
                    },
                    diff: vec![],
                    guardrail: GuardrailDecision {
                        risk_level: RiskLevel::High,
                        blocked: true,
                        reason: Some("too risky".to_string()),
                        target_summary: None,
                    },
                });
                state.query.set_delete_refresh_target(0, None, 1);
                state.confirm_dialog.open(
                    "",
                    "",
                    ConfirmIntent::ExecuteWrite {
                        sql: "UPDATE t SET x=1".to_string(),
                        blocked: true,
                    },
                );

                route(
                    &mut state,
                    Action::ConfirmDialogConfirm,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert!(state.result_interaction.pending_write_preview().is_none());
                assert!(state.query.pending_delete_refresh_target().is_none());
            }

            #[test]
            fn csv_export_returns_export_effect() {
                let (mut state, run_id) = csv_state_with_current_run();
                open_confirm_intent(&mut state, rerunnable_csv_intent(run_id));

                let effects = confirm_effects(&mut state);
                assert_eq!(effects.len(), 1);
                let Effect::ExportCsv {
                    dsn,
                    run_id: effect_run_id,
                    query,
                    file_name,
                } = &effects[0]
                else {
                    panic!("expected rerunnable CSV export effect");
                };

                assert_eq!(dsn, CSV_TEST_DSN);
                assert_eq!(*effect_run_id, run_id);
                assert_eq!(query, "SELECT 1");
                assert_eq!(file_name, CSV_TEST_FILE);
            }

            #[test]
            fn cached_csv_export_returns_export_from_cache_effect() {
                let (mut state, run_id) = csv_state_with_current_run();
                open_confirm_intent(&mut state, cached_csv_intent(run_id));

                let effects = confirm_effects(&mut state);
                assert_eq!(effects.len(), 1);
                assert_cached_export_effect(&effects[0], run_id);
            }

            #[test]
            fn csv_export_ignores_mismatched_dsn() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                let _ = state
                    .session
                    .begin_connecting("postgres://localhost/current");
                let _ = state.query.begin_running(Instant::now());
                state.confirm_dialog.open(
                    "",
                    "",
                    ConfirmIntent::CsvExportRerunnable {
                        dsn: "postgres://localhost/stale".to_string(),
                        run_id: 1,
                        export_query: "SELECT 1".to_string(),
                        file_name: "test.csv".to_string(),
                    },
                );

                let effects = route(
                    &mut state,
                    Action::ConfirmDialogConfirm,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert!(effects.is_empty());
            }

            #[test]
            fn csv_export_ignores_mismatched_run_id() {
                let (mut state, _) = csv_state_with_current_run();
                open_confirm_intent(&mut state, rerunnable_csv_intent(2));

                let effects = confirm_effects(&mut state);
                assert!(effects.is_empty());
            }

            #[test]
            fn cached_csv_export_ignores_mismatched_run_id() {
                let (mut state, _) = csv_state_with_current_run();
                open_confirm_intent(&mut state, cached_csv_intent(2));

                let effects = confirm_effects(&mut state);
                assert!(effects.is_empty());
            }

            #[test]
            fn disable_read_only_confirm_sets_read_only_false() {
                let mut state = create_test_state();
                state.session.enable_read_only();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state
                    .confirm_dialog
                    .open("", "", ConfirmIntent::DisableReadOnly);

                let effects = route(
                    &mut state,
                    Action::ConfirmDialogConfirm,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert!(!state.session.is_read_only());
                assert_eq!(state.input_mode(), InputMode::Normal);
                assert!(effects.is_empty());
            }

            #[test]
            fn none_intent_confirm_does_not_panic() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);

                let effects = route(
                    &mut state,
                    Action::ConfirmDialogConfirm,
                    Instant::now(),
                    &AppServices::stub(),
                );

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

                route(
                    &mut state,
                    Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.confirm_dialog.preview_scroll, 1);
            }

            #[test]
            fn up_decrements_offset() {
                let mut state = state_with_scrollable_preview();
                state.confirm_dialog.preview_scroll = 5;

                route(
                    &mut state,
                    Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Up,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.confirm_dialog.preview_scroll, 4);
            }

            #[test]
            fn up_clamps_at_zero() {
                let mut state = state_with_scrollable_preview();
                state.confirm_dialog.preview_scroll = 0;

                route(
                    &mut state,
                    Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Up,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.confirm_dialog.preview_scroll, 0);
            }

            #[test]
            fn down_clamps_at_max() {
                let mut state = state_with_scrollable_preview();
                state.confirm_dialog.preview_scroll = 15;

                route(
                    &mut state,
                    Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                    &AppServices::stub(),
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
            use crate::policy::write::write_guardrails::{
                GuardrailDecision, RiskLevel, TargetSummary, WriteOperation, WritePreview,
            };

            #[test]
            fn quit_no_connection_restores_connection_setup_synchronously() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state
                    .confirm_dialog
                    .open("", "", ConfirmIntent::QuitNoConnection);

                let effects = route(
                    &mut state,
                    Action::ConfirmDialogCancel,
                    Instant::now(),
                    &AppServices::stub(),
                );

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

                let effects = route(
                    &mut state,
                    Action::ConfirmDialogCancel,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::CellEdit);
                assert!(effects.is_empty());
                assert!(state.result_interaction.pending_write_preview().is_none());
            }

            #[test]
            fn delete_preview_preserves_staged_rows() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state.result_interaction.activate_cell(4, 2);
                state.result_interaction.stage_row(4);
                state.result_interaction.set_write_preview(WritePreview {
                    operation: WriteOperation::Delete,
                    sql: "DELETE FROM users WHERE id = 4".to_string(),
                    target_summary: TargetSummary {
                        schema: "public".to_string(),
                        table: "users".to_string(),
                        key_values: vec![],
                    },
                    diff: vec![],
                    guardrail: GuardrailDecision {
                        risk_level: RiskLevel::Low,
                        blocked: false,
                        reason: None,
                        target_summary: None,
                    },
                });
                state.confirm_dialog.open(
                    "Confirm DELETE".to_string(),
                    "DELETE FROM users WHERE id = 4".to_string(),
                    ConfirmIntent::ExecuteWrite {
                        sql: "DELETE FROM users WHERE id = 4".to_string(),
                        blocked: false,
                    },
                );

                let effects = cancel_effects(&mut state);

                assert!(effects.is_empty());
                assert!(state.result_interaction.staged_delete_rows().contains(&4));
                assert_eq!(state.result_interaction.selection().row(), Some(4));
                assert_eq!(state.result_interaction.selection().cell(), Some(2));
            }

            #[test]
            fn current_csv_export_cancel_marks_query_idle() {
                let (mut state, run_id) = csv_state_with_current_run();
                open_confirm_intent(&mut state, rerunnable_csv_intent(run_id));

                let effects = cancel_effects(&mut state);
                assert!(effects.is_empty());
                assert!(!state.query.is_running());
            }

            #[test]
            fn current_cached_csv_export_cancel_marks_query_idle() {
                let (mut state, run_id) = csv_state_with_current_run();
                open_confirm_intent(&mut state, cached_csv_intent(run_id));

                let effects = cancel_effects(&mut state);
                assert!(effects.is_empty());
                assert!(!state.query.is_running());
            }

            #[test]
            fn stale_csv_export_cancel_keeps_current_run() {
                let (mut state, stale_run_id, current_run_id) = csv_state_with_stale_run();
                open_confirm_intent(&mut state, rerunnable_csv_intent(stale_run_id));

                let effects = cancel_effects(&mut state);
                assert!(effects.is_empty());
                assert!(state.query.is_running());
                assert!(state.query.is_current_run(current_run_id));
            }

            #[test]
            fn stale_cached_csv_export_cancel_keeps_current_run() {
                let (mut state, stale_run_id, current_run_id) = csv_state_with_stale_run();
                open_confirm_intent(&mut state, cached_csv_intent(stale_run_id));

                let effects = cancel_effects(&mut state);
                assert!(effects.is_empty());
                assert!(state.query.is_running());
                assert!(state.query.is_current_run(current_run_id));
            }

            #[test]
            fn none_intent_cancel_does_not_panic() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);

                let effects = route(
                    &mut state,
                    Action::ConfirmDialogCancel,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert!(effects.is_empty());
            }
        }
    }

    mod query_history_picker {
        use super::*;
        use crate::domain::query_history::{
            QueryHistoryEntry, QueryHistoryScope, QueryResultStatus,
        };

        fn make_entry(query: &str, conn_id: &ConnectionId) -> QueryHistoryEntry {
            QueryHistoryEntry::new_with_database(
                query.to_string(),
                "2026-03-13T12:00:00Z".to_string(),
                conn_id.clone(),
                None,
                QueryResultStatus::Success,
                None,
            )
        }

        fn scope(conn_id: &ConnectionId, database: Option<&str>) -> QueryHistoryScope {
            QueryHistoryScope::new(conn_id.clone(), database.map(str::to_owned))
        }

        fn connected_state() -> AppState {
            let mut state = AppState::new("test-project".to_string());
            state.session.activate_connection_with_dsn(
                &ConnectionId::from_string("test-conn"),
                "test",
                DatabaseType::PostgreSQL,
                "postgres://localhost/test",
            );
            state
        }

        fn mysql_connected_state(database: &str) -> AppState {
            let mut state = AppState::new("test-project".to_string());
            state.session.activate_connection_with_target(
                &ConnectionId::from_string("mysql-conn"),
                "mysql",
                DatabaseType::MySQL,
                &format!("mysql://localhost/{database}"),
                Some(database),
            );
            state.session.mark_probe_connected();
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
                state.session.clear_connection();

                let effects = route(
                    &mut state,
                    Action::OpenModal(ModalKind::QueryHistoryPicker),
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::Normal);
                assert!(effects.is_empty());
            }

            #[test]
            fn open_when_running_is_noop() {
                let mut state = connected_state();
                let _ = state.query.begin_running(Instant::now());

                let effects = route(
                    &mut state,
                    Action::OpenModal(ModalKind::QueryHistoryPicker),
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::Normal);
                assert!(effects.is_empty());
            }
        }

        mod lifecycle {
            use super::*;

            #[test]
            fn open_from_normal_sets_mode_and_emits_load_effect() {
                let mut state = connected_state();

                let effects = route(
                    &mut state,
                    Action::OpenModal(ModalKind::QueryHistoryPicker),
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::QueryHistoryPicker);
                assert_eq!(state.modal.return_destination(), InputMode::Normal);
                assert_eq!(effects.len(), 1);
                assert!(matches!(&effects[0], Effect::LoadQueryHistory { .. }));
            }

            #[test]
            fn open_while_already_open_is_noop() {
                let mut state = connected_state();
                state.modal.push_mode(InputMode::QueryHistoryPicker);

                let effects = route(
                    &mut state,
                    Action::OpenModal(ModalKind::QueryHistoryPicker),
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::QueryHistoryPicker);
                assert_eq!(state.modal.return_destination(), InputMode::Normal);
                assert!(effects.is_empty());
            }

            #[test]
            fn open_for_mysql_captures_selected_database_in_scope() {
                let mut state = mysql_connected_state("analytics");

                let effects = route(
                    &mut state,
                    Action::OpenModal(ModalKind::QueryHistoryPicker),
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert!(matches!(
                    &effects[0],
                    Effect::LoadQueryHistory { scope, .. }
                        if scope.database.as_deref() == Some("analytics")
                ));
            }

            #[test]
            fn close_restores_origin_mode() {
                let mut state = connected_state();
                state.modal.set_mode(InputMode::SqlModal);
                state.modal.push_mode(InputMode::QueryHistoryPicker);

                let effects = route(
                    &mut state,
                    Action::CloseModal(ModalKind::QueryHistoryPicker),
                    Instant::now(),
                    &AppServices::stub(),
                );

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

                let effects = route(
                    &mut state,
                    Action::QueryHistoryLoaded(scope(&conn_id, None), entries),
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.query_history_picker.entries().len(), 1);
                assert!(effects.is_empty());
            }

            #[test]
            fn loaded_ignores_stale_connection() {
                let mut state = connected_state();
                state.modal.set_mode(InputMode::QueryHistoryPicker);
                let stale_conn = ConnectionId::from_string("old-conn");
                let entries = vec![make_entry("SELECT 1", &stale_conn)];

                route(
                    &mut state,
                    Action::QueryHistoryLoaded(scope(&stale_conn, None), entries),
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert!(state.query_history_picker.entries().is_empty());
            }

            #[test]
            fn loaded_ignores_stale_mysql_database_scope() {
                let mut state = mysql_connected_state("analytics");
                state.modal.set_mode(InputMode::QueryHistoryPicker);
                let conn_id = ConnectionId::from_string("mysql-conn");
                let stale_scope = scope(&conn_id, Some("app"));
                let entries = vec![QueryHistoryEntry::new_with_database(
                    "SELECT 1".to_string(),
                    "2026-03-13T12:00:00Z".to_string(),
                    conn_id,
                    Some("app".to_string()),
                    QueryResultStatus::Success,
                    None,
                )];

                route(
                    &mut state,
                    Action::QueryHistoryLoaded(stale_scope, entries),
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert!(state.query_history_picker.entries().is_empty());
            }

            #[test]
            fn loaded_ignores_when_picker_closed() {
                let mut state = connected_state();
                let conn_id = ConnectionId::from_string("test-conn");
                let entries = vec![make_entry("SELECT 1", &conn_id)];

                route(
                    &mut state,
                    Action::QueryHistoryLoaded(scope(&conn_id, None), entries),
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert!(state.query_history_picker.entries().is_empty());
            }

            #[test]
            fn load_failed_sets_persistent_error() {
                let mut state = connected_state();
                state.modal.set_mode(InputMode::QueryHistoryPicker);
                let now = Instant::now();

                route(
                    &mut state,
                    Action::QueryHistoryLoadFailed(
                        scope(&ConnectionId::from_string("test-conn"), None),
                        "IO error: disk error".to_string(),
                    ),
                    now,
                    &AppServices::stub(),
                );

                assert_eq!(
                    state.messages.last_error.as_deref(),
                    Some("IO error: disk error")
                );
                assert!(state.messages.expires_at.is_none());
            }

            #[test]
            fn load_failed_ignored_when_picker_not_active() {
                let mut state = connected_state();
                let now = Instant::now();

                route(
                    &mut state,
                    Action::QueryHistoryLoadFailed(
                        scope(&ConnectionId::from_string("test-conn"), None),
                        "IO error: stale error".to_string(),
                    ),
                    now,
                    &AppServices::stub(),
                );

                assert!(state.messages.last_error.is_none());
            }

            #[test]
            fn load_failed_ignored_when_connection_mismatches() {
                let mut state = connected_state();
                state.modal.set_mode(InputMode::QueryHistoryPicker);
                let now = Instant::now();

                route(
                    &mut state,
                    Action::QueryHistoryLoadFailed(
                        scope(&ConnectionId::from_string("old-conn"), None),
                        "IO error: stale error".to_string(),
                    ),
                    now,
                    &AppServices::stub(),
                );

                assert!(state.messages.last_error.is_none());
            }
        }

        mod filter_and_selection {
            use super::*;
            use rstest::rstest;

            #[test]
            fn filter_input_resets_selection() {
                let mut state = connected_state();
                state.query_history_picker.set_selection_for_test(5);

                let effects = edit_text(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::QueryHistoryFilter,
                        ch: 'a',
                    },
                    Instant::now(),
                );

                assert_eq!(state.query_history_picker.selected(), 0);
                assert_eq!(state.query_history_picker.filter_input().content(), "a");
                assert!(effects.is_empty());
            }

            #[rstest]
            #[case::next_increments(vec!["SELECT 1", "SELECT 2"], 0, ListMotion::Next, 1)]
            #[case::next_clamps_at_end(vec!["SELECT 1"], 0, ListMotion::Next, 0)]
            #[case::previous_decrements(Vec::<&str>::new(), 1, ListMotion::Previous, 0)]
            fn selection_moves_to_expected_index(
                #[case] queries: Vec<&str>,
                #[case] initial_selection: usize,
                #[case] motion: ListMotion,
                #[case] expected_selection: usize,
            ) {
                let mut state = connected_state();
                let test_conn = ConnectionId::from_string("test-conn");
                let entries = queries
                    .iter()
                    .map(|query| make_entry(query, &test_conn))
                    .collect::<Vec<_>>();
                state.query_history_picker.replace_entries(&entries);
                state
                    .query_history_picker
                    .set_selection_for_test(initial_selection);

                route(
                    &mut state,
                    Action::ListSelect {
                        target: ListTarget::QueryHistory,
                        motion,
                    },
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.query_history_picker.selected(), expected_selection);
            }
        }

        mod confirm_selection {
            use super::*;
            use crate::model::sql_editor::completion::{CompletionCandidate, CompletionKind};
            use crate::model::sql_editor::modal::SqlModalStatus;

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

                route(
                    &mut state,
                    Action::QueryHistoryConfirmSelection,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.sql_modal.editor.cursor(), expected_chars);
            }

            #[test]
            fn confirm_from_normal_opens_sql_modal_with_query() {
                let mut state = connected_state();
                enter_query_history(&mut state, InputMode::Normal);
                let test_conn = ConnectionId::from_string("test-conn");
                state
                    .query_history_picker
                    .replace_entries(&[make_entry("SELECT * FROM users", &test_conn)]);

                let effects = route(
                    &mut state,
                    Action::QueryHistoryConfirmSelection,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::SqlModal);
                assert_eq!(state.sql_modal.editor.content(), "SELECT * FROM users");
                assert!(matches!(state.sql_modal.status(), SqlModalStatus::Normal));
                assert!(effects.is_empty());
            }

            #[test]
            fn confirm_from_sql_modal_overwrites_editor_content() {
                let mut state = connected_state();
                enter_query_history(&mut state, InputMode::SqlModal);
                state.sql_modal.editor.set_content("old query".to_string());
                state.sql_modal.enter_editing();
                state.sql_modal.apply_completion_update(
                    &[
                        CompletionCandidate {
                            text: "stale".to_string(),
                            kind: CompletionKind::Keyword,
                            score: 1,
                        },
                        CompletionCandidate {
                            text: "older".to_string(),
                            kind: CompletionKind::Keyword,
                            score: 0,
                        },
                    ],
                    0,
                    true,
                );
                state.sql_modal.completion_next();
                let test_conn = ConnectionId::from_string("test-conn");
                state
                    .query_history_picker
                    .replace_entries(&[make_entry("new query", &test_conn)]);

                route(
                    &mut state,
                    Action::QueryHistoryConfirmSelection,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::SqlModal);
                assert_eq!(state.sql_modal.editor.content(), "new query");
                assert!(matches!(state.sql_modal.status(), SqlModalStatus::Normal));
                assert!(!state.sql_modal.completion().visible);
                assert!(state.sql_modal.completion().candidates.is_empty());
                assert_eq!(state.sql_modal.completion().selected_index, 0);
            }

            #[test]
            fn confirm_with_empty_entries_is_noop() {
                let mut state = connected_state();
                enter_query_history(&mut state, InputMode::Normal);

                route(
                    &mut state,
                    Action::QueryHistoryConfirmSelection,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::Normal);
            }

            #[test]
            fn confirm_during_pending_mysql_probe_does_not_open_sql_modal() {
                let mut state = connected_state();
                enter_query_history(&mut state, InputMode::Normal);
                let test_conn = ConnectionId::from_string("test-conn");
                state
                    .query_history_picker
                    .replace_entries(&[make_entry("SELECT * FROM users", &test_conn)]);
                let target_id = ConnectionId::from_string("mysql-conn");
                let _ = state.session.begin_mysql_connection_probe(
                    &target_id,
                    "mysql",
                    "mysql://localhost/app",
                    Some("app"),
                );

                let effects = route(
                    &mut state,
                    Action::QueryHistoryConfirmSelection,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.input_mode(), InputMode::QueryHistoryPicker);
                assert_eq!(state.query_history_picker.entries().len(), 1);
                assert_eq!(
                    state.messages.last_error.as_deref(),
                    Some("Connection switch in progress")
                );
                assert!(effects.is_empty());
            }
        }
    }
}
