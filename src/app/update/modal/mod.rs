mod base;
mod confirm_dialog;
mod er_picker;
mod help;
mod query_history;
mod settings;

use std::time::Instant;

use crate::model::app_state::AppState;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub fn dispatch_modal(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    base::reduce_base_lifecycle(state, action, now)
        .or_else(|| settings::reduce_settings(state, action, now))
        .or_else(|| help::reduce_help(state, action, now))
        .or_else(|| confirm_dialog::reduce_confirm_dialog(state, action, now))
        .or_else(|| er_picker::reduce_er_picker(state, action, now))
        .or_else(|| query_history::reduce_query_history_picker(state, action, now))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::cmd::effect::Effect;
    use crate::model::shared::confirm_dialog::ConfirmIntent;
    use crate::model::shared::input_mode::InputMode;
    use crate::ports::outbound::AppSettings;
    use crate::update::action::{
        InputTarget, ListMotion, ListTarget, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget,
    };

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

            let effects =
                super::dispatch_modal(&mut state, &Action::Escape, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(effects.is_empty());
        }

        #[test]
        fn escape_passes_for_modal_with_specific_close_action() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::SqlModal);

            let result = super::dispatch_modal(&mut state, &Action::Escape, Instant::now());

            assert!(result.is_pass());
            assert_eq!(state.input_mode(), InputMode::SqlModal);
        }
    }

    mod help {
        use super::*;

        fn open_help(state: &mut AppState) {
            super::dispatch_modal(
                state,
                &Action::OpenModal(ModalKind::CommandPalette),
                Instant::now(),
            );
            super::dispatch_modal(state, &Action::ToggleModal(ModalKind::Help), Instant::now());
            assert_eq!(state.input_mode(), InputMode::Help);
        }

        #[test]
        fn escape_returns_to_help_origin_with_filter() {
            let mut state = create_test_state();
            open_help(&mut state);
            state.ui.help.insert_filter_char('c');

            let effects = super::dispatch_modal(
                &mut state,
                &Action::CloseModal(ModalKind::Help),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::CommandPalette);
            assert!(state.ui.help.filter().content().is_empty());
            assert!(effects.is_empty());
        }

        #[test]
        fn escape_returns_to_help_origin_when_filter_is_empty() {
            let mut state = create_test_state();
            open_help(&mut state);

            let effects = super::dispatch_modal(
                &mut state,
                &Action::CloseModal(ModalKind::Help),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::CommandPalette);
            assert!(effects.is_empty());
        }

        #[test]
        fn filter_text_actions_update_help_state() {
            let mut state = create_test_state();
            open_help(&mut state);

            let input_effects = super::dispatch_modal(
                &mut state,
                &Action::TextInput {
                    target: InputTarget::HelpFilter,
                    ch: 'k',
                },
                Instant::now(),
            )
            .unwrap();
            let backspace_effects = super::dispatch_modal(
                &mut state,
                &Action::TextBackspace {
                    target: InputTarget::HelpFilter,
                },
                Instant::now(),
            )
            .unwrap();

            assert!(state.ui.help.filter().content().is_empty());
            assert!(input_effects.is_empty());
            assert!(backspace_effects.is_empty());
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

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.input_mode(), InputMode::Settings);
                assert_eq!(state.settings.previous_theme(), ThemeId::Light);
                assert_eq!(state.settings.selected_theme(), ThemeId::Light);
                assert!(effects.is_empty());
            }

            #[test]
            fn navigates_without_applying() {
                let mut state = create_test_state();
                super::dispatch_modal(
                    &mut state,
                    &Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                );

                super::dispatch_modal(&mut state, &Action::SettingsSelectNext, Instant::now());

                assert_eq!(state.settings.selected_theme(), ThemeId::Light);
                assert_eq!(state.ui.theme_id(), ThemeId::Default);
            }

            #[rstest]
            #[case(Action::SettingsCancel)]
            #[case(Action::CloseModal(ModalKind::Settings))]
            fn cancel_discards_selection(#[case] action: Action) {
                let mut state = create_test_state();
                super::dispatch_modal(
                    &mut state,
                    &Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                );
                super::dispatch_modal(&mut state, &Action::SettingsSelectNext, Instant::now());

                let effects = super::dispatch_modal(&mut state, &action, Instant::now())
                    .into_effects()
                    .expect("reducer should handle action");

                assert_eq!(state.input_mode(), InputMode::Normal);
                assert_eq!(state.settings.selected_theme(), ThemeId::Default);
                assert_eq!(state.ui.theme_id(), ThemeId::Default);
                assert!(effects.is_empty());
            }

            #[test]
            fn apply_emits_settings_save_effect() {
                let mut state = create_test_state();
                super::dispatch_modal(
                    &mut state,
                    &Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                );
                super::dispatch_modal(&mut state, &Action::SettingsSelectNext, Instant::now());

                let effects =
                    super::dispatch_modal(&mut state, &Action::SettingsApply, Instant::now())
                        .into_effects()
                        .expect("reducer should handle action");

                assert_eq!(state.input_mode(), InputMode::Settings);
                assert_eq!(state.ui.theme_id(), ThemeId::Default);
                assert!(matches!(
                    effects.as_slice(),
                    [Effect::SaveSettings { settings }]
                        if settings.theme_id == ThemeId::Light && settings.er_browser.is_none()
                ));
            }

            #[test]
            fn apply_emits_er_browser_save_effect() {
                let mut state = create_test_state();
                super::dispatch_modal(
                    &mut state,
                    &Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                );
                super::dispatch_modal(&mut state, &Action::SettingsNextSection, Instant::now());
                super::dispatch_modal(&mut state, &Action::SettingsSelectNext, Instant::now());

                let effects =
                    super::dispatch_modal(&mut state, &Action::SettingsApply, Instant::now())
                        .into_effects()
                        .expect("reducer should handle action");

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
                super::dispatch_modal(
                    &mut state,
                    &Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                );
                super::dispatch_modal(&mut state, &Action::SettingsNextSection, Instant::now());
                super::dispatch_modal(
                    &mut state,
                    &Action::SettingsStartCustomBrowserEdit,
                    Instant::now(),
                );
                super::dispatch_modal(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::SettingsErBrowser,
                        ch: 'B',
                    },
                    Instant::now(),
                );

                let effects =
                    super::dispatch_modal(&mut state, &Action::SettingsApply, Instant::now())
                        .into_effects()
                        .expect("reducer should handle action");

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
                super::dispatch_modal(
                    &mut state,
                    &Action::OpenModal(ModalKind::Settings),
                    Instant::now(),
                );
                super::dispatch_modal(&mut state, &Action::SettingsSelectNext, Instant::now());
                super::dispatch_modal(&mut state, &Action::SettingsNextSection, Instant::now());
                super::dispatch_modal(&mut state, &Action::SettingsSelectNext, Instant::now());

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::SettingsSaved(AppSettings {
                        theme_id: ThemeId::Light,
                        er_browser: Some("Google Chrome".to_string()),
                    }),
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.ui.theme_id(), ThemeId::Light);
                assert_eq!(state.settings.previous_theme(), ThemeId::Light);
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

            let effects = super::dispatch_modal(
                &mut state,
                &Action::SettingsSaveFailed(crate::ports::outbound::SettingsStoreError::Io(
                    std::sync::Arc::new(std::io::Error::other("disk full")),
                )),
                Instant::now(),
            )
            .unwrap();

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

        mod confirm {
            use super::*;

            #[test]
            fn quit_no_connection_sets_should_quit() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state
                    .confirm_dialog
                    .open("", "", ConfirmIntent::QuitNoConnection);

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::ConfirmDialogConfirm,
                    Instant::now(),
                )
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

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::ConfirmDialogConfirm,
                    Instant::now(),
                )
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
                let effects = super::dispatch_modal(&mut state, &Action::ConfirmDialogConfirm, now)
                    .into_effects()
                    .expect("reducer should handle action");

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

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::ConfirmDialogConfirm,
                    Instant::now(),
                )
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

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::ConfirmDialogConfirm,
                    Instant::now(),
                )
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

                super::dispatch_modal(&mut state, &Action::ConfirmDialogConfirm, Instant::now())
                    .into_effects()
                    .expect("reducer should handle action");

                assert!(state.result_interaction.pending_write_preview().is_none());
                assert!(state.query.pending_delete_refresh_target().is_none());
            }

            #[test]
            fn csv_export_returns_export_effect() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state.session.dsn = Some("postgres://localhost/test".to_string());
                let _ = state.query.begin_running(Instant::now());
                state.confirm_dialog.open(
                    "",
                    "",
                    ConfirmIntent::CsvExport {
                        dsn: "postgres://localhost/test".to_string(),
                        run_id: 1,
                        export_query: "SELECT 1".to_string(),
                        file_name: "test.csv".to_string(),
                        row_count: Some(200_000),
                    },
                );

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::ConfirmDialogConfirm,
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(effects.len(), 1);
                assert!(matches!(&effects[0], Effect::ExportCsv { .. }));
            }

            #[test]
            fn csv_export_ignores_mismatched_dsn() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state.session.dsn = Some("postgres://localhost/current".to_string());
                let _ = state.query.begin_running(Instant::now());
                state.confirm_dialog.open(
                    "",
                    "",
                    ConfirmIntent::CsvExport {
                        dsn: "postgres://localhost/stale".to_string(),
                        run_id: 1,
                        export_query: "SELECT 1".to_string(),
                        file_name: "test.csv".to_string(),
                        row_count: Some(200_000),
                    },
                );

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::ConfirmDialogConfirm,
                    Instant::now(),
                )
                .unwrap();

                assert!(effects.is_empty());
            }

            #[test]
            fn csv_export_ignores_mismatched_run_id() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state.session.dsn = Some("postgres://localhost/test".to_string());
                let _ = state.query.begin_running(Instant::now());
                state.confirm_dialog.open(
                    "",
                    "",
                    ConfirmIntent::CsvExport {
                        dsn: "postgres://localhost/test".to_string(),
                        run_id: 2,
                        export_query: "SELECT 1".to_string(),
                        file_name: "test.csv".to_string(),
                        row_count: Some(200_000),
                    },
                );

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::ConfirmDialogConfirm,
                    Instant::now(),
                )
                .unwrap();

                assert!(effects.is_empty());
            }

            #[test]
            fn disable_read_only_confirm_sets_read_only_false() {
                let mut state = create_test_state();
                state.session.read_only = true;
                enter_confirm_dialog(&mut state, InputMode::Normal);
                state
                    .confirm_dialog
                    .open("", "", ConfirmIntent::DisableReadOnly);

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::ConfirmDialogConfirm,
                    Instant::now(),
                )
                .unwrap();

                assert!(!state.session.read_only);
                assert_eq!(state.input_mode(), InputMode::Normal);
                assert!(effects.is_empty());
            }

            #[test]
            fn none_intent_confirm_does_not_panic() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::ConfirmDialogConfirm,
                    Instant::now(),
                )
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

                super::dispatch_modal(
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

                super::dispatch_modal(
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

                super::dispatch_modal(
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

                super::dispatch_modal(
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
                    super::dispatch_modal(&mut state, &Action::ConfirmDialogCancel, Instant::now())
                        .into_effects()
                        .expect("reducer should handle action");

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
                    super::dispatch_modal(&mut state, &Action::ConfirmDialogCancel, Instant::now())
                        .into_effects()
                        .expect("reducer should handle action");

                assert_eq!(state.input_mode(), InputMode::CellEdit);
                assert!(effects.is_empty());
                assert!(state.result_interaction.pending_write_preview().is_none());
            }

            #[test]
            fn none_intent_cancel_does_not_panic() {
                let mut state = create_test_state();
                enter_confirm_dialog(&mut state, InputMode::Normal);

                let effects =
                    super::dispatch_modal(&mut state, &Action::ConfirmDialogCancel, Instant::now())
                        .into_effects()
                        .expect("reducer should handle action");

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

                let effects = super::dispatch_modal(
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
                let _ = state.query.begin_running(Instant::now());

                let effects = super::dispatch_modal(
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

                let effects = super::dispatch_modal(
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
            fn open_while_already_open_is_noop() {
                let mut state = connected_state();
                state.modal.push_mode(InputMode::QueryHistoryPicker);

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::OpenModal(ModalKind::QueryHistoryPicker),
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.input_mode(), InputMode::QueryHistoryPicker);
                assert_eq!(state.modal.return_destination(), InputMode::Normal);
                assert!(effects.is_empty());
            }

            #[test]
            fn close_restores_origin_mode() {
                let mut state = connected_state();
                state.modal.set_mode(InputMode::SqlModal);
                state.modal.push_mode(InputMode::QueryHistoryPicker);

                let effects = super::dispatch_modal(
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

                let effects = super::dispatch_modal(
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

                super::dispatch_modal(
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

                super::dispatch_modal(
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

                super::dispatch_modal(
                    &mut state,
                    &Action::QueryHistoryLoadFailed(
                        crate::domain::ConnectionId::from_string("test-conn"),
                        QueryHistoryError::Io(Arc::new(std::io::Error::other("disk error"))),
                    ),
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

                super::dispatch_modal(
                    &mut state,
                    &Action::QueryHistoryLoadFailed(
                        crate::domain::ConnectionId::from_string("test-conn"),
                        QueryHistoryError::Io(Arc::new(std::io::Error::other("stale error"))),
                    ),
                    now,
                )
                .unwrap();

                assert!(state.messages.last_error.is_none());
            }

            #[test]
            fn load_failed_ignored_when_connection_mismatches() {
                let mut state = connected_state();
                state.modal.set_mode(InputMode::QueryHistoryPicker);
                let now = Instant::now();

                super::dispatch_modal(
                    &mut state,
                    &Action::QueryHistoryLoadFailed(
                        ConnectionId::from_string("old-conn"),
                        QueryHistoryError::Io(Arc::new(std::io::Error::other("stale error"))),
                    ),
                    now,
                )
                .unwrap();

                assert!(state.messages.last_error.is_none());
            }

            #[test]
            fn append_failed_does_not_set_error() {
                let mut state = connected_state();
                let now = Instant::now();

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::QueryHistoryAppendFailed(QueryHistoryError::Io(Arc::new(
                        std::io::Error::other("write error"),
                    ))),
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
                state.query_history_picker.set_selection_for_test(5);

                let effects = super::dispatch_modal(
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

                super::dispatch_modal(
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

                super::dispatch_modal(
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

                super::dispatch_modal(
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

                super::dispatch_modal(
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
                state
                    .query_history_picker
                    .replace_entries(&[make_entry("SELECT * FROM users", &test_conn)]);

                let effects = super::dispatch_modal(
                    &mut state,
                    &Action::QueryHistoryConfirmSelection,
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.input_mode(), InputMode::SqlModal);
                assert_eq!(state.sql_modal.editor.content(), "SELECT * FROM users");
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
                state.sql_modal.editor.set_content("old query".to_string());
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

                super::dispatch_modal(
                    &mut state,
                    &Action::QueryHistoryConfirmSelection,
                    Instant::now(),
                )
                .unwrap();

                assert_eq!(state.input_mode(), InputMode::SqlModal);
                assert_eq!(state.sql_modal.editor.content(), "new query");
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

                super::dispatch_modal(
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
