mod completion;
mod editing;
mod helpers;
mod high_risk;
mod mode;
mod submit;
mod yank;

use std::time::Instant;

use crate::model::app_state::AppState;
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub fn dispatch_sql_modal(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> DispatchResult {
    completion::reduce_completion(state, action, now)
        .or_else(|| editing::reduce_editing(state, action, now))
        .or_else(|| mode::reduce_mode(state, action, now))
        .or_else(|| submit::reduce_submit(state, action, now))
        .or_else(|| high_risk::reduce_high_risk_confirmation(state, action, now))
        .or_else(|| yank::reduce_yank(state, action, now, services))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::effect::Effect;
    use crate::model::shared::flash_timer::FlashId;
    use crate::model::shared::input_mode::InputMode;
    use crate::model::shared::text_input::{TextInputLike, TextInputState};
    use crate::model::sql_editor::modal::{SqlModalStatus, SqlModalTab};
    use crate::policy::write::write_guardrails::RiskLevel;
    use crate::update::action::{CursorMove, InputTarget, ModalKind};
    use std::time::Instant;

    fn reduce_sql_modal(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
        super::dispatch_sql_modal(state, action, now, &crate::services::AppServices::stub())
    }

    fn sql_modal_state() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::SqlModal);
        state
    }

    mod paste {
        use super::*;

        fn editing_state() -> AppState {
            let mut state = sql_modal_state();
            state.sql_modal.set_status_for_test(SqlModalStatus::Editing);
            state
        }

        #[test]
        fn inserts_at_cursor() {
            let mut state = editing_state();
            state
                .sql_modal
                .editor
                .set_content_with_cursor("SELCT".to_string(), 3);

            reduce_sql_modal(&mut state, &Action::Paste("E".to_string()), Instant::now());

            assert_eq!(state.sql_modal.editor.content(), "SELECT");
        }

        #[test]
        fn preserves_newlines() {
            let mut state = editing_state();

            reduce_sql_modal(
                &mut state,
                &Action::Paste("SELECT\n*\nFROM".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.editor.content(), "SELECT\n*\nFROM");
        }

        #[test]
        fn normalizes_crlf() {
            let mut state = editing_state();

            reduce_sql_modal(
                &mut state,
                &Action::Paste("a\r\nb".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.editor.content(), "a\nb");
        }

        #[test]
        fn advances_cursor() {
            let mut state = editing_state();
            state
                .sql_modal
                .editor
                .set_content_with_cursor("AB".to_string(), 1);

            reduce_sql_modal(
                &mut state,
                &Action::Paste("XYZ".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.editor.cursor(), 4); // 1 + 3
        }

        #[test]
        fn dismisses_completion() {
            let mut state = editing_state();
            state.sql_modal.completion_mut_for_test().visible = true;

            reduce_sql_modal(&mut state, &Action::Paste("x".to_string()), Instant::now());

            assert!(!state.sql_modal.completion().visible);
        }

        #[test]
        fn advances_cursor_with_multibyte() {
            let mut state = editing_state();
            state
                .sql_modal
                .editor
                .set_content_with_cursor("ab".to_string(), 1);

            reduce_sql_modal(
                &mut state,
                &Action::Paste("日本語".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.editor.content(), "a日本語b");
            assert_eq!(state.sql_modal.editor.cursor(), 4); // 1 + 3
        }

        #[test]
        fn confirming_high_is_ignored() {
            let mut state = editing_state();
            state
                .sql_modal
                .editor
                .set_content("DROP TABLE users".to_string());
            state
                .sql_modal
                .set_status_for_test(SqlModalStatus::ConfirmingHigh {
                    decision: crate::policy::write::write_guardrails::AdhocRiskDecision {
                        risk_level: RiskLevel::High,
                        label: "DROP",
                    },
                    input: TextInputState::default(),
                    target_name: Some("users".to_string()),
                });

            reduce_sql_modal(
                &mut state,
                &Action::Paste("injected".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.editor.content(), "DROP TABLE users");
            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh { .. }
            ));
        }
    }

    mod scrolling {
        use super::*;
        use crate::update::action::CursorMove;

        #[test]
        fn moves_down_without_scrolling_while_cursor_stays_inside_visible_rows() {
            let mut state = sql_modal_state();
            state.ui.terminal_height = 20;
            state.sql_modal.set_status_for_test(SqlModalStatus::Normal);
            state
                .sql_modal
                .editor
                .set_content_with_cursor("0\n1\n2\n3\n4\n5\n6\n7".to_string(), 0);

            for _ in 0..7 {
                reduce_sql_modal(
                    &mut state,
                    &Action::TextMoveCursor {
                        target: InputTarget::SqlModal,
                        direction: CursorMove::Down,
                    },
                    Instant::now(),
                );
            }

            assert_eq!(state.sql_modal.editor.cursor_to_position(), (7, 0));
            assert_eq!(state.sql_modal.editor.scroll_row(), 0);
        }

        #[test]
        fn scrolls_once_cursor_moves_past_visible_rows() {
            let mut state = sql_modal_state();
            state.ui.terminal_height = 20;
            state.sql_modal.set_status_for_test(SqlModalStatus::Normal);
            state
                .sql_modal
                .editor
                .set_content_with_cursor("0\n1\n2\n3\n4\n5\n6\n7\n8".to_string(), 0);

            for _ in 0..8 {
                reduce_sql_modal(
                    &mut state,
                    &Action::TextMoveCursor {
                        target: InputTarget::SqlModal,
                        direction: CursorMove::Down,
                    },
                    Instant::now(),
                );
            }

            assert_eq!(state.sql_modal.editor.cursor_to_position(), (8, 0));
            assert_eq!(state.sql_modal.editor.scroll_row(), 1);
        }
    }

    mod confirming_high {
        use super::*;
        use crate::policy::write::write_guardrails::AdhocRiskDecision;
        use crate::update::action::CursorMove;

        fn confirming_high_state(content: &str, target: Option<&str>) -> AppState {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content(content.to_string());
            state
                .sql_modal
                .set_status_for_test(SqlModalStatus::ConfirmingHigh {
                    decision: AdhocRiskDecision {
                        risk_level: RiskLevel::High,
                        label: "DROP",
                    },
                    input: TextInputState::default(),
                    target_name: target.map(ToString::to_string),
                });
            state
        }

        #[test]
        fn submit_high_risk_drop_enters_confirming_high() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("DROP TABLE users".to_string());

            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh {
                    target_name: Some(name),
                    ..
                } if name == "users"
            ));
        }

        #[test]
        fn submit_other_executes_immediately() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("SELECT * INTO backup FROM users".to_string());
            state.session.dsn = Some("postgres://test".to_string());

            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(state.sql_modal.status(), SqlModalStatus::Running));
        }

        #[test]
        fn submit_unsupported_executes_immediately() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("COPY users FROM '/tmp/data.csv'".to_string());
            state.session.dsn = Some("postgres://test".to_string());

            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(state.sql_modal.status(), SqlModalStatus::Running));
        }

        #[test]
        fn submit_medium_risk_executes_immediately() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("UPDATE users SET x=1 WHERE id=1".to_string());
            state.session.dsn = Some("postgres://test".to_string());

            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(state.sql_modal.status(), SqlModalStatus::Running));
        }

        #[test]
        fn submit_medium_risk_without_dsn_sets_error() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("UPDATE users SET x=1 WHERE id=1".to_string());

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(state.sql_modal.status(), SqlModalStatus::Error));
            assert_eq!(
                state.sql_modal.last_adhoc_error(),
                Some("No active connection")
            );
            assert!(effects.is_handled_and(Vec::is_empty));
        }

        #[test]
        fn high_risk_input_appends_char() {
            let mut state = confirming_high_state("DROP TABLE users", Some("users"));

            reduce_sql_modal(
                &mut state,
                &Action::TextInput {
                    target: InputTarget::SqlModalHighRisk,
                    ch: 'u',
                },
                Instant::now(),
            );

            if let SqlModalStatus::ConfirmingHigh { input, .. } = state.sql_modal.status() {
                assert_eq!(input.content(), "u");
            } else {
                panic!("expected ConfirmingHigh");
            }
        }

        #[test]
        fn high_risk_backspace_removes_char() {
            let mut state = confirming_high_state("DROP TABLE users", Some("users"));
            reduce_sql_modal(
                &mut state,
                &Action::TextInput {
                    target: InputTarget::SqlModalHighRisk,
                    ch: 'a',
                },
                Instant::now(),
            );
            reduce_sql_modal(
                &mut state,
                &Action::TextInput {
                    target: InputTarget::SqlModalHighRisk,
                    ch: 'b',
                },
                Instant::now(),
            );

            reduce_sql_modal(
                &mut state,
                &Action::TextBackspace {
                    target: InputTarget::SqlModalHighRisk,
                },
                Instant::now(),
            );

            if let SqlModalStatus::ConfirmingHigh { input, .. } = state.sql_modal.status() {
                assert_eq!(input.content(), "a");
            } else {
                panic!("expected ConfirmingHigh");
            }
        }

        #[test]
        fn high_risk_confirm_executes_on_match() {
            let mut state = confirming_high_state("DROP TABLE users", Some("users"));
            state.session.dsn = Some("postgres://test".to_string());
            for c in "users".chars() {
                reduce_sql_modal(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::SqlModalHighRisk,
                        ch: c,
                    },
                    Instant::now(),
                );
            }

            let effects = reduce_sql_modal(
                &mut state,
                &Action::SqlModalHighRiskConfirmExecute,
                Instant::now(),
            );

            assert!(matches!(state.sql_modal.status(), SqlModalStatus::Running));
            assert!(
                effects.is_handled_and(|e| e
                    .iter()
                    .any(|ef| matches!(ef, Effect::ExecuteAdhoc { .. })))
            );
        }

        #[test]
        fn high_risk_confirm_without_dsn_sets_error() {
            let mut state = confirming_high_state("DROP TABLE users", Some("users"));
            for c in "users".chars() {
                reduce_sql_modal(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::SqlModalHighRisk,
                        ch: c,
                    },
                    Instant::now(),
                );
            }

            let effects = reduce_sql_modal(
                &mut state,
                &Action::SqlModalHighRiskConfirmExecute,
                Instant::now(),
            );

            assert!(matches!(state.sql_modal.status(), SqlModalStatus::Error));
            assert_eq!(
                state.sql_modal.last_adhoc_error(),
                Some("No active connection")
            );
            assert!(effects.is_handled_and(Vec::is_empty));
        }

        #[test]
        fn high_risk_confirm_blocked_on_mismatch() {
            let mut state = confirming_high_state("DROP TABLE users", Some("users"));
            reduce_sql_modal(
                &mut state,
                &Action::TextInput {
                    target: InputTarget::SqlModalHighRisk,
                    ch: 'x',
                },
                Instant::now(),
            );

            reduce_sql_modal(
                &mut state,
                &Action::SqlModalHighRiskConfirmExecute,
                Instant::now(),
            );

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh { .. }
            ));
        }

        #[test]
        fn high_risk_confirm_blocked_when_no_target() {
            let mut state = confirming_high_state("DROP TABLE users", None);

            reduce_sql_modal(
                &mut state,
                &Action::SqlModalHighRiskConfirmExecute,
                Instant::now(),
            );

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh { .. }
            ));
        }

        #[test]
        fn cancel_from_confirming_high_returns_to_normal() {
            let mut state = confirming_high_state("DROP TABLE users", Some("users"));

            reduce_sql_modal(&mut state, &Action::SqlModalCancelConfirm, Instant::now());

            assert!(matches!(state.sql_modal.status(), SqlModalStatus::Normal));
        }

        #[test]
        fn high_risk_move_cursor_works() {
            let mut state = confirming_high_state("DROP TABLE users", Some("users"));
            for c in "ab".chars() {
                reduce_sql_modal(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::SqlModalHighRisk,
                        ch: c,
                    },
                    Instant::now(),
                );
            }

            reduce_sql_modal(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::SqlModalHighRisk,
                    direction: CursorMove::Left,
                },
                Instant::now(),
            );

            if let SqlModalStatus::ConfirmingHigh { input, .. } = state.sql_modal.status() {
                assert_eq!(input.cursor(), 1);
            } else {
                panic!("expected ConfirmingHigh");
            }
        }

        #[test]
        fn submit_delete_no_where_enters_confirming_high() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("DELETE FROM users".to_string());

            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh {
                    target_name: Some(name),
                    ..
                } if name == "users"
            ));
        }

        #[test]
        fn submit_update_no_where_enters_confirming_high() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("UPDATE users SET x=1".to_string());

            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh {
                    target_name: Some(name),
                    ..
                } if name == "users"
            ));
        }

        #[test]
        fn submit_truncate_enters_confirming_high() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("TRUNCATE users".to_string());

            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh {
                    target_name: Some(name),
                    ..
                } if name == "users"
            ));
        }

        #[test]
        fn submit_drop_schema_qualified_preserves_full_name() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("DROP TABLE my_schema.very_long_table_name".to_string());

            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh {
                    target_name: Some(name),
                    ..
                } if name == "my_schema.very_long_table_name"
            ));
        }

        #[test]
        fn high_risk_confirm_matches_full_name_not_truncated() {
            let full_name = "my_schema.very_long_table_name";
            let mut state =
                confirming_high_state(&format!("DROP TABLE {full_name}"), Some(full_name));
            state.session.dsn = Some("postgres://test".to_string());
            for c in full_name.chars() {
                reduce_sql_modal(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::SqlModalHighRisk,
                        ch: c,
                    },
                    Instant::now(),
                );
            }

            let effects = reduce_sql_modal(
                &mut state,
                &Action::SqlModalHighRiskConfirmExecute,
                Instant::now(),
            );

            assert!(matches!(state.sql_modal.status(), SqlModalStatus::Running));
            assert!(
                effects.is_handled_and(|e| e
                    .iter()
                    .any(|ef| matches!(ef, Effect::ExecuteAdhoc { .. })))
            );
        }
    }

    mod read_only_guard {
        use super::*;

        #[test]
        fn read_only_blocks_write_query_in_sql_modal() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::SqlModal);
            state
                .sql_modal
                .editor
                .set_content("DELETE FROM users WHERE id = 1".to_string());
            state.session.dsn = Some("postgres://localhost/test".to_string());
            state.session.read_only = true;

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Error);
            assert_eq!(
                state.sql_modal.last_adhoc_error(),
                Some("Read-only mode: write operations are disabled")
            );
        }

        #[test]
        fn read_only_reject_clears_prior_success() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::SqlModal);
            state.session.dsn = Some("postgres://localhost/test".to_string());
            state.session.read_only = true;

            // Simulate a prior adhoc success
            state.sql_modal.finish_adhoc_success(
                crate::model::sql_editor::modal::AdhocSuccessSnapshot {
                    command_tag: None,
                    row_count: 5,
                    execution_time_ms: 10,
                },
            );
            assert!(state.sql_modal.last_adhoc_success().is_some());

            // Now submit a write query in read-only mode
            state
                .sql_modal
                .editor
                .set_content("DELETE FROM users WHERE id = 1".to_string());
            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Error);
            assert!(state.sql_modal.last_adhoc_success().is_none());
            assert!(state.sql_modal.last_adhoc_error().is_some());
        }

        #[test]
        fn read_only_allows_select_query() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::SqlModal);
            state.sql_modal.editor.set_content("SELECT 1".to_string());
            state.session.dsn = Some("postgres://localhost/test".to_string());
            state.session.read_only = true;

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(!effects.is_empty());
            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
        }
    }

    mod confirmation_flow {
        use super::*;
        use crate::policy::write::write_guardrails::RiskLevel;

        fn modal_state_with_query(query: &str) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::SqlModal);
            state.sql_modal.editor.set_content(query.to_string());
            state.session.dsn = Some("postgres://localhost/test".to_string());
            state
        }

        #[test]
        fn submit_select_executes_immediately() {
            let mut state = modal_state_with_query("SELECT 1");

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ExecuteAdhoc { .. }))
            );
        }

        #[test]
        fn submit_insert_executes_immediately() {
            let mut state = modal_state_with_query("INSERT INTO t VALUES (1)");

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ExecuteAdhoc { .. }))
            );
        }

        #[test]
        fn submit_delete_without_where_enters_confirming_high() {
            let mut state = modal_state_with_query("DELETE FROM users");

            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh { decision, .. }
                    if decision.risk_level == RiskLevel::High
            ));
        }
    }

    mod normal_insert_mode {
        use super::*;

        #[test]
        fn append_insert_moves_to_line_end_and_transitions_to_editing() {
            let mut state = sql_modal_state();
            state.sql_modal.set_status_for_test(SqlModalStatus::Normal);
            state
                .sql_modal
                .editor
                .set_content_with_cursor("abc\ndef".to_string(), 1);

            reduce_sql_modal(&mut state, &Action::SqlModalAppendInsert, Instant::now());

            assert_eq!(state.sql_modal.editor.cursor_to_position(), (0, 3));
            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Editing);
        }

        #[test]
        fn enter_insert_transitions_to_editing() {
            let mut state = sql_modal_state();

            reduce_sql_modal(&mut state, &Action::SqlModalEnterInsert, Instant::now());

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Editing);
        }

        #[test]
        fn enter_normal_transitions_to_normal() {
            let mut state = sql_modal_state();
            state.sql_modal.set_status_for_test(SqlModalStatus::Editing);
            state.sql_modal.completion_mut_for_test().visible = true;

            reduce_sql_modal(&mut state, &Action::SqlModalEnterNormal, Instant::now());

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Normal);
            assert!(!state.sql_modal.completion().visible);
        }

        #[test]
        fn vertical_move_after_edit_uses_current_column() {
            let mut state = sql_modal_state();
            state.sql_modal.set_status_for_test(SqlModalStatus::Normal);
            state
                .sql_modal
                .editor
                .set_content_with_cursor("abcdefghij\nxy\nabcdefghij".to_string(), 8);

            reduce_sql_modal(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::SqlModal,
                    direction: CursorMove::Down,
                },
                Instant::now(),
            );
            assert_eq!(state.sql_modal.editor.cursor_to_position(), (1, 2));

            reduce_sql_modal(&mut state, &Action::SqlModalEnterInsert, Instant::now());
            reduce_sql_modal(
                &mut state,
                &Action::TextInput {
                    target: InputTarget::SqlModal,
                    ch: 'z',
                },
                Instant::now(),
            );
            reduce_sql_modal(&mut state, &Action::SqlModalEnterNormal, Instant::now());
            reduce_sql_modal(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::SqlModal,
                    direction: CursorMove::Down,
                },
                Instant::now(),
            );

            assert_eq!(state.sql_modal.editor.cursor_to_position(), (2, 3));
        }

        #[test]
        fn yank_empty_content_is_noop() {
            let mut state = sql_modal_state();

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }

        #[test]
        fn yank_non_empty_emits_copy_effect() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content("SELECT 1".to_string());

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(effects.len(), 1);
            assert!(
                matches!(&effects[0], Effect::CopyToClipboard { content, .. } if content == "SELECT 1")
            );
        }

        #[test]
        fn yank_success_sets_flash() {
            let mut state = sql_modal_state();
            let now = Instant::now();

            reduce_sql_modal(&mut state, &Action::SqlModalYankSuccess, now);

            assert!(state.flash_timers.is_active(FlashId::SqlModal, now));
        }

        #[test]
        fn open_sql_modal_starts_in_normal() {
            let mut state = AppState::new("test".to_string());

            reduce_sql_modal(
                &mut state,
                &Action::OpenModal(ModalKind::SqlModal),
                Instant::now(),
            );

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Normal);
        }

        #[test]
        fn open_sql_modal_resets_active_tab_to_sql() {
            let mut state = AppState::new("test".to_string());
            state.sql_modal.set_active_tab(SqlModalTab::Plan);

            reduce_sql_modal(
                &mut state,
                &Action::OpenModal(ModalKind::SqlModal),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.active_tab(), SqlModalTab::Sql);
        }

        #[test]
        fn ignored_in_normal_mode() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content("original".to_string());

            reduce_sql_modal(
                &mut state,
                &Action::Paste("injected".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.editor.content(), "original");
        }
    }

    mod yank {
        use super::*;
        use crate::domain::explain_plan::ExplainPlan;
        use crate::model::explain_context::{CompareSlot, SlotSource};

        fn make_slot(raw: &str, is_analyze: bool, ms: u64, source: SlotSource) -> CompareSlot {
            CompareSlot {
                plan: ExplainPlan {
                    raw_text: raw.to_string(),
                    top_node_type: None,
                    total_cost: None,
                    estimated_rows: None,
                    is_analyze,
                    execution_time_ms: ms,
                },
                query_snippet: "SELECT 1".to_string(),
                full_query: "SELECT 1".to_string(),
                source,
            }
        }

        #[test]
        fn sql_tab_yank_copies_content() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content("SELECT 1".to_string());
            state.sql_modal.set_active_tab(SqlModalTab::Sql);

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(effects.len(), 1);
            assert!(
                matches!(&effects[0], Effect::CopyToClipboard { content, .. } if content == "SELECT 1")
            );
        }

        #[test]
        fn sql_tab_yank_empty_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content(String::new());
            state.sql_modal.set_active_tab(SqlModalTab::Sql);

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }

        #[test]
        fn plan_tab_yank_copies_plan_text() {
            let mut state = sql_modal_state();
            state.sql_modal.set_active_tab(SqlModalTab::Plan);
            state.explain.plan_text = Some("Seq Scan on users".to_string());

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(effects.len(), 1);
            assert!(
                matches!(&effects[0], Effect::CopyToClipboard { content, .. } if content == "Seq Scan on users")
            );
        }

        #[test]
        fn plan_tab_yank_no_plan_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.set_active_tab(SqlModalTab::Plan);
            state.explain.plan_text = None;

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }

        #[test]
        fn plan_tab_yank_error_state_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.set_active_tab(SqlModalTab::Plan);
            state.explain.plan_text = None;
            state.explain.error = Some("syntax error".to_string());

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }

        #[test]
        fn compare_tab_yank_both_slots() {
            let mut state = sql_modal_state();
            state.sql_modal.set_active_tab(SqlModalTab::Compare);
            state.explain.left = Some(make_slot("Seq Scan", false, 420, SlotSource::AutoPrevious));
            state.explain.right = Some(make_slot("Index Scan", true, 50, SlotSource::AutoLatest));

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(effects.len(), 1);
            if let Effect::CopyToClipboard { content, .. } = &effects[0] {
                // Verdict section comes first
                assert!(content.starts_with("Unavailable\n"));
                // Then slot plans
                assert!(content.contains("--- Left: Previous (EXPLAIN, 0.42s) ---"));
                assert!(content.contains("Seq Scan"));
                assert!(content.contains("--- Right: Latest (ANALYZE, 0.05s) ---"));
                assert!(content.contains("Index Scan"));
            } else {
                panic!("expected CopyToClipboard");
            }
        }

        #[test]
        fn both_auto_slots_yank_returns_distinguishable_headers() {
            let mut state = sql_modal_state();
            state.sql_modal.set_active_tab(SqlModalTab::Compare);
            state.explain.left = Some(make_slot("Seq Scan", false, 300, SlotSource::AutoPrevious));
            state.explain.right = Some(make_slot("Index Scan", false, 100, SlotSource::AutoLatest));

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(effects.len(), 1);
            if let Effect::CopyToClipboard { content, .. } = &effects[0] {
                assert!(content.contains("--- Left: Previous"));
                assert!(content.contains("--- Right: Latest"));
            } else {
                panic!("expected CopyToClipboard");
            }
        }

        #[test]
        fn compare_tab_yank_right_only_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.set_active_tab(SqlModalTab::Compare);
            state.explain.left = None;
            state.explain.right = Some(make_slot("Index Scan", false, 100, SlotSource::AutoLatest));

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }

        #[test]
        fn compare_tab_yank_left_only_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.set_active_tab(SqlModalTab::Compare);
            state.explain.left = Some(make_slot("Seq Scan", false, 200, SlotSource::AutoPrevious));
            state.explain.right = None;

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }

        #[test]
        fn compare_tab_yank_empty_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.set_active_tab(SqlModalTab::Compare);
            state.explain.left = None;
            state.explain.right = None;

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }

        #[test]
        fn compare_tab_yank_includes_verdict_with_reasons() {
            let mut state = sql_modal_state();
            state.sql_modal.set_active_tab(SqlModalTab::Compare);
            // Use parseable EXPLAIN output so compare_plans produces a real verdict
            state.explain.left = Some(CompareSlot {
                plan: ExplainPlan {
                    raw_text: "Seq Scan on users  (cost=0.00..100.00 rows=10 width=32)".to_string(),
                    top_node_type: Some("Seq Scan".to_string()),
                    total_cost: Some(100.0),
                    estimated_rows: Some(10),
                    is_analyze: false,
                    execution_time_ms: 420,
                },
                query_snippet: "SELECT *".to_string(),
                full_query: "SELECT * FROM users".to_string(),
                source: SlotSource::AutoPrevious,
            });
            state.explain.right = Some(CompareSlot {
                plan: ExplainPlan {
                    raw_text: "Index Scan using idx on users  (cost=0.00..5.00 rows=1 width=32)"
                        .to_string(),
                    top_node_type: Some("Index Scan".to_string()),
                    total_cost: Some(5.0),
                    estimated_rows: Some(1),
                    is_analyze: false,
                    execution_time_ms: 50,
                },
                query_snippet: "SELECT *".to_string(),
                full_query: "SELECT * FROM users WHERE id=1".to_string(),
                source: SlotSource::AutoLatest,
            });

            let effects = reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(effects.len(), 1);
            if let Effect::CopyToClipboard { content, .. } = &effects[0] {
                assert!(content.starts_with("Improved\n"));
                assert!(content.contains("Total cost:"));
                assert!(content.contains("--- Left: Previous"));
                assert!(content.contains("--- Right: Latest"));
            } else {
                panic!("expected CopyToClipboard");
            }
        }
    }
}
