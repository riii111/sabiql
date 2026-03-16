//! SQL modal sub-reducer: SQL editing and completion.

use std::time::{Duration, Instant};

use crate::app::action::{Action, CursorMove};
use crate::app::adhoc_risk::{ConfirmationType, MultiStatementDecision, evaluate_multi_statement};
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::reducers::{char_count, char_to_byte_index};
use crate::app::sql_modal_context::{HIGH_RISK_INPUT_VISIBLE_WIDTH, SqlModalStatus};
use crate::app::state::AppState;
use crate::app::statement_classifier;
use crate::app::text_input::TextInputState;
use crate::app::write_guardrails::{AdhocRiskDecision, RiskLevel, evaluate_adhoc_risk};

/// Handles SQL modal editing and completion actions.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_sql_modal(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> Option<Vec<Effect>> {
    match action {
        // Completion navigation
        Action::CompletionNext => {
            if !state.sql_modal.completion.candidates.is_empty() {
                let max = state.sql_modal.completion.candidates.len() - 1;
                state.sql_modal.completion.selected_index =
                    if state.sql_modal.completion.selected_index >= max {
                        0
                    } else {
                        state.sql_modal.completion.selected_index + 1
                    };
            }
            Some(vec![])
        }
        Action::CompletionPrev => {
            if !state.sql_modal.completion.candidates.is_empty() {
                let max = state.sql_modal.completion.candidates.len() - 1;
                state.sql_modal.completion.selected_index =
                    if state.sql_modal.completion.selected_index == 0 {
                        max
                    } else {
                        state.sql_modal.completion.selected_index - 1
                    };
            }
            Some(vec![])
        }
        Action::CompletionDismiss => {
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion_debounce = None;
            Some(vec![])
        }

        // Clipboard paste
        Action::Paste(text) if state.modal.active_mode() == InputMode::SqlModal => {
            if matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh { .. }
            ) {
                return Some(vec![]);
            }
            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert_str(byte_idx, &normalized);
            state.sql_modal.cursor += normalized.chars().count();
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            state.sql_modal.set_status(SqlModalStatus::Editing);
            Some(vec![])
        }

        // Text editing
        Action::SqlModalInput(c) => {
            state.sql_modal.set_status(SqlModalStatus::Editing);
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert(byte_idx, *c);
            state.sql_modal.cursor += 1;
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            Some(vec![])
        }
        Action::SqlModalBackspace => {
            state.sql_modal.set_status(SqlModalStatus::Editing);
            if state.sql_modal.cursor > 0 {
                state.sql_modal.cursor -= 1;
                let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
                state.sql_modal.content.remove(byte_idx);
            }
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            Some(vec![])
        }
        Action::SqlModalDelete => {
            state.sql_modal.set_status(SqlModalStatus::Editing);
            let total_chars = char_count(&state.sql_modal.content);
            if state.sql_modal.cursor < total_chars {
                let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
                state.sql_modal.content.remove(byte_idx);
            }
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            Some(vec![])
        }
        Action::SqlModalNewLine => {
            state.sql_modal.set_status(SqlModalStatus::Editing);
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert(byte_idx, '\n');
            state.sql_modal.cursor += 1;
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            Some(vec![])
        }
        Action::SqlModalTab => {
            state.sql_modal.set_status(SqlModalStatus::Editing);
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert_str(byte_idx, "    ");
            state.sql_modal.cursor += 4;
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            Some(vec![])
        }
        Action::SqlModalMoveCursor(movement) => {
            let content = &state.sql_modal.content;
            let cursor = state.sql_modal.cursor;
            let total_chars = char_count(content);

            let lines: Vec<(usize, usize)> = {
                let mut result = Vec::new();
                let mut start = 0;
                for line in content.split('\n') {
                    let len = line.chars().count();
                    result.push((start, len));
                    start += len + 1;
                }
                result
            };

            let (current_line, current_col) = {
                let mut line_idx = 0;
                let mut col = cursor;
                for (i, (start, len)) in lines.iter().enumerate() {
                    if cursor >= *start && cursor <= start + len {
                        line_idx = i;
                        col = cursor - start;
                        break;
                    }
                }
                (line_idx, col)
            };

            state.sql_modal.cursor = match movement {
                CursorMove::Left => cursor.saturating_sub(1),
                CursorMove::Right => (cursor + 1).min(total_chars),
                CursorMove::Home => lines.get(current_line).map(|(s, _)| *s).unwrap_or(0),
                CursorMove::End => lines
                    .get(current_line)
                    .map(|(s, l)| s + l)
                    .unwrap_or(total_chars),
                CursorMove::Up => {
                    if current_line == 0 {
                        cursor
                    } else {
                        let (prev_start, prev_len) = lines[current_line - 1];
                        prev_start + current_col.min(prev_len)
                    }
                }
                CursorMove::Down => {
                    if current_line + 1 >= lines.len() {
                        cursor
                    } else {
                        let (next_start, next_len) = lines[current_line + 1];
                        next_start + current_col.min(next_len)
                    }
                }
            };
            Some(vec![])
        }
        Action::SqlModalClear => {
            state.sql_modal.content.clear();
            state.sql_modal.cursor = 0;
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion.candidates.clear();
            Some(vec![])
        }

        // Modal open/submit
        Action::OpenSqlModal => {
            state.modal.set_mode(InputMode::SqlModal);
            state.sql_modal.set_status(SqlModalStatus::Editing);
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion.candidates.clear();
            state.sql_modal.completion.selected_index = 0;
            state.sql_modal.completion_debounce = None;
            if !state.sql_modal.is_prefetch_started() && state.session.metadata().is_some() {
                Some(vec![Effect::DispatchActions(vec![
                    Action::StartPrefetchAll,
                ])])
            } else {
                Some(vec![])
            }
        }
        Action::SqlModalSubmit => {
            let query = state.sql_modal.content.trim().to_string();
            if query.is_empty() {
                return Some(vec![]);
            }
            state.sql_modal.completion.visible = false;

            match evaluate_multi_statement(&query) {
                MultiStatementDecision::Block { reason } => {
                    state.sql_modal.mark_adhoc_error(reason);
                    Some(vec![])
                }
                MultiStatementDecision::Allow { risk, .. } => {
                    let label = multi_statement_label(&query);
                    let decision = AdhocRiskDecision {
                        risk_level: risk.risk_level,
                        label,
                    };
                    // In read-only mode, block non-Immediate (write) queries
                    if state.session.read_only
                        && !matches!(risk.confirmation, ConfirmationType::Immediate)
                    {
                        state.sql_modal.mark_adhoc_error(
                            "Read-only mode: write operations are disabled".to_string(),
                        );
                        return Some(vec![]);
                    }
                    match risk.confirmation {
                        ConfirmationType::Immediate => {
                            state.sql_modal.set_status(SqlModalStatus::Running);
                            Some(adhoc_effects(state, query))
                        }
                        ConfirmationType::Enter => {
                            state
                                .sql_modal
                                .set_status(SqlModalStatus::Confirming(decision));
                            Some(vec![])
                        }
                        ConfirmationType::TableNameInput { target } => {
                            state.sql_modal.set_status(SqlModalStatus::ConfirmingHigh {
                                decision,
                                input: TextInputState::default(),
                                target_name: Some(target),
                            });
                            Some(vec![])
                        }
                    }
                }
            }
        }
        Action::SqlModalConfirmExecute => {
            if matches!(state.sql_modal.status(), SqlModalStatus::Confirming(_)) {
                let query = state.sql_modal.content.trim().to_string();
                state.sql_modal.set_status(SqlModalStatus::Running);
                Some(adhoc_effects(state, query))
            } else {
                None
            }
        }
        Action::SqlModalCancelConfirm => {
            if matches!(
                state.sql_modal.status(),
                SqlModalStatus::Confirming(_) | SqlModalStatus::ConfirmingHigh { .. }
            ) {
                state.sql_modal.set_status(SqlModalStatus::Editing);
                Some(vec![])
            } else {
                None
            }
        }

        // HIGH risk confirmation input
        Action::SqlModalHighRiskInput(c) => {
            if let Some(input) = state.sql_modal.confirming_high_input_mut() {
                input.insert_char(*c);
                input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
            }
            Some(vec![])
        }
        Action::SqlModalHighRiskBackspace => {
            if let Some(input) = state.sql_modal.confirming_high_input_mut() {
                input.backspace();
                input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
            }
            Some(vec![])
        }
        Action::SqlModalHighRiskMoveCursor(movement) => {
            if let Some(input) = state.sql_modal.confirming_high_input_mut() {
                input.move_cursor(*movement);
                input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
            }
            Some(vec![])
        }
        Action::SqlModalHighRiskConfirmExecute => {
            // `matches!` + flag instead of `if let` because the immutable borrow
            // from pattern matching must end before we can mutate `state.sql_modal.status`.
            let matched = matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh {
                    target_name,
                    input,
                    ..
                } if target_name.as_ref().is_some_and(|n| input.content() == n)
            );
            if matched {
                let query = state.sql_modal.content.trim().to_string();
                state.sql_modal.set_status(SqlModalStatus::Running);
                if let Some(dsn) = &state.session.dsn {
                    return Some(vec![Effect::ExecuteAdhoc {
                        dsn: dsn.clone(),
                        query,
                        read_only: state.session.read_only,
                    }]);
                }
            }
            Some(vec![])
        }

        // Completion accept
        Action::CompletionAccept => {
            if state.sql_modal.completion.visible
                && !state.sql_modal.completion.candidates.is_empty()
            {
                let selected_idx = state.sql_modal.completion.selected_index;
                let trigger_pos = state.sql_modal.completion.trigger_position;
                let candidates = std::mem::take(&mut state.sql_modal.completion.candidates);

                if let Some(candidate) = candidates.into_iter().nth(selected_idx) {
                    let start_byte = char_to_byte_index(&state.sql_modal.content, trigger_pos);
                    let end_byte =
                        char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
                    state.sql_modal.content.drain(start_byte..end_byte);
                    state
                        .sql_modal
                        .content
                        .insert_str(start_byte, &candidate.text);
                    state.sql_modal.cursor = trigger_pos + candidate.text.chars().count();
                }
                state.sql_modal.completion.visible = false;
                state.sql_modal.completion_debounce = None;
            }
            Some(vec![])
        }

        // Completion trigger/update
        Action::CompletionTrigger => Some(vec![Effect::TriggerCompletion]),
        Action::CompletionUpdated {
            candidates,
            trigger_position,
            visible,
        } => {
            state.sql_modal.completion.candidates = candidates.clone();
            state.sql_modal.completion.trigger_position = *trigger_position;
            state.sql_modal.completion.visible = *visible;
            state.sql_modal.completion.selected_index = 0;
            Some(vec![])
        }

        _ => None,
    }
}

/// Pick the label from the highest-risk individual statement for the confirmation UI.
fn multi_statement_label(sql: &str) -> &'static str {
    use crate::app::adhoc_risk::split_statements;
    let mut worst_level = RiskLevel::Low;
    let mut worst_label = "SQL";
    for stmt in split_statements(sql) {
        let kind = statement_classifier::classify(&stmt);
        let d = evaluate_adhoc_risk(&kind);
        if d.risk_level > worst_level || (d.risk_level == worst_level && d.label != "SQL") {
            worst_level = d.risk_level;
            worst_label = d.label;
        }
    }
    worst_label
}

fn adhoc_effects(state: &AppState, query: String) -> Vec<Effect> {
    match &state.session.dsn {
        Some(dsn) => vec![Effect::ExecuteAdhoc {
            dsn: dsn.clone(),
            query,
            read_only: state.session.read_only,
        }],
        None => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    mod paste {
        use super::*;

        fn sql_modal_state() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::SqlModal);
            state
        }

        #[test]
        fn paste_inserts_at_cursor() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "SELCT".to_string();
            state.sql_modal.cursor = 3;

            reduce_sql_modal(&mut state, &Action::Paste("E".to_string()), Instant::now());

            assert_eq!(state.sql_modal.content, "SELECT");
        }

        #[test]
        fn paste_preserves_newlines() {
            let mut state = sql_modal_state();

            reduce_sql_modal(
                &mut state,
                &Action::Paste("SELECT\n*\nFROM".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.content, "SELECT\n*\nFROM");
        }

        #[test]
        fn paste_normalizes_crlf() {
            let mut state = sql_modal_state();

            reduce_sql_modal(
                &mut state,
                &Action::Paste("a\r\nb".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.content, "a\nb");
        }

        #[test]
        fn paste_advances_cursor() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "AB".to_string();
            state.sql_modal.cursor = 1;

            reduce_sql_modal(
                &mut state,
                &Action::Paste("XYZ".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.cursor, 4); // 1 + 3
        }

        #[test]
        fn paste_dismisses_completion() {
            let mut state = sql_modal_state();
            state.sql_modal.completion.visible = true;

            reduce_sql_modal(&mut state, &Action::Paste("x".to_string()), Instant::now());

            assert!(!state.sql_modal.completion.visible);
        }

        #[test]
        fn paste_with_multibyte() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "ab".to_string();
            state.sql_modal.cursor = 1;

            reduce_sql_modal(
                &mut state,
                &Action::Paste("日本語".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.content, "a日本語b");
            assert_eq!(state.sql_modal.cursor, 4); // 1 + 3
        }

        #[test]
        fn paste_in_confirming_high_is_ignored() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "DROP TABLE users".to_string();
            state.sql_modal.set_status(SqlModalStatus::ConfirmingHigh {
                decision: crate::app::write_guardrails::AdhocRiskDecision {
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

            assert_eq!(state.sql_modal.content, "DROP TABLE users");
            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh { .. }
            ));
        }
    }

    mod confirming_high {
        use super::*;
        use crate::app::write_guardrails::AdhocRiskDecision;

        fn sql_modal_state() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::SqlModal);
            state
        }

        fn confirming_high_state(content: &str, target: Option<&str>) -> AppState {
            let mut state = sql_modal_state();
            state.sql_modal.content = content.to_string();
            state.sql_modal.set_status(SqlModalStatus::ConfirmingHigh {
                decision: AdhocRiskDecision {
                    risk_level: RiskLevel::High,
                    label: "DROP",
                },
                input: TextInputState::default(),
                target_name: target.map(|s| s.to_string()),
            });
            state
        }

        #[test]
        fn submit_high_risk_drop_enters_confirming_high() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "DROP TABLE users".to_string();

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
        fn submit_other_falls_back_to_confirming_high() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "GRANT ALL ON users TO role1".to_string();

            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::Confirming(d) if d.risk_level == RiskLevel::High
            ));
        }

        #[test]
        fn submit_unsupported_falls_back_to_confirming_high() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "COPY users FROM '/tmp/data.csv'".to_string();

            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::Confirming(d) if d.risk_level == RiskLevel::High
            ));
        }

        #[test]
        fn submit_medium_risk_stays_confirming() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "UPDATE users SET x=1 WHERE id=1".to_string();

            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::Confirming(d) if d.risk_level == RiskLevel::Medium
            ));
        }

        #[test]
        fn high_risk_input_appends_char() {
            let mut state = confirming_high_state("DROP TABLE users", Some("users"));

            reduce_sql_modal(
                &mut state,
                &Action::SqlModalHighRiskInput('u'),
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
                &Action::SqlModalHighRiskInput('a'),
                Instant::now(),
            );
            reduce_sql_modal(
                &mut state,
                &Action::SqlModalHighRiskInput('b'),
                Instant::now(),
            );

            reduce_sql_modal(
                &mut state,
                &Action::SqlModalHighRiskBackspace,
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
                    &Action::SqlModalHighRiskInput(c),
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
                effects
                    .is_some_and(|e| e.iter().any(|ef| matches!(ef, Effect::ExecuteAdhoc { .. })))
            );
        }

        #[test]
        fn high_risk_confirm_blocked_on_mismatch() {
            let mut state = confirming_high_state("DROP TABLE users", Some("users"));
            reduce_sql_modal(
                &mut state,
                &Action::SqlModalHighRiskInput('x'),
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
        fn cancel_from_confirming_high_returns_to_editing() {
            let mut state = confirming_high_state("DROP TABLE users", Some("users"));

            reduce_sql_modal(&mut state, &Action::SqlModalCancelConfirm, Instant::now());

            assert!(matches!(state.sql_modal.status(), SqlModalStatus::Editing));
        }

        #[test]
        fn high_risk_move_cursor_works() {
            let mut state = confirming_high_state("DROP TABLE users", Some("users"));
            for c in "ab".chars() {
                reduce_sql_modal(
                    &mut state,
                    &Action::SqlModalHighRiskInput(c),
                    Instant::now(),
                );
            }

            reduce_sql_modal(
                &mut state,
                &Action::SqlModalHighRiskMoveCursor(CursorMove::Left),
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
            state.sql_modal.content = "DELETE FROM users".to_string();

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
            state.sql_modal.content = "UPDATE users SET x=1".to_string();

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
            state.sql_modal.content = "TRUNCATE users".to_string();

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
            state.sql_modal.content = "DROP TABLE my_schema.very_long_table_name".to_string();

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
                confirming_high_state(&format!("DROP TABLE {}", full_name), Some(full_name));
            state.session.dsn = Some("postgres://test".to_string());
            for c in full_name.chars() {
                reduce_sql_modal(
                    &mut state,
                    &Action::SqlModalHighRiskInput(c),
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
                effects
                    .is_some_and(|e| e.iter().any(|ef| matches!(ef, Effect::ExecuteAdhoc { .. })))
            );
        }
    }

    mod read_only_guard {
        use super::*;

        #[test]
        fn read_only_blocks_write_query_in_sql_modal() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::SqlModal);
            state.sql_modal.content = "DELETE FROM users WHERE id = 1".to_string();
            state.session.dsn = Some("postgres://localhost/test".to_string());
            state.session.read_only = true;

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now()).unwrap();

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
            state.sql_modal.mark_adhoc_success(
                crate::app::sql_modal_context::AdhocSuccessSnapshot {
                    command_tag: None,
                    row_count: 5,
                    execution_time_ms: 10,
                },
            );
            assert!(state.sql_modal.last_adhoc_success().is_some());

            // Now submit a write query in read-only mode
            state.sql_modal.content = "DELETE FROM users WHERE id = 1".to_string();
            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now()).unwrap();

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Error);
            assert!(state.sql_modal.last_adhoc_success().is_none());
            assert!(state.sql_modal.last_adhoc_error().is_some());
        }

        #[test]
        fn read_only_allows_select_query() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::SqlModal);
            state.sql_modal.content = "SELECT 1".to_string();
            state.session.dsn = Some("postgres://localhost/test".to_string());
            state.session.read_only = true;

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now()).unwrap();

            assert!(!effects.is_empty());
            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
        }
    }

    mod confirmation_flow {
        use super::*;
        use crate::app::write_guardrails::RiskLevel;

        fn modal_state_with_query(query: &str) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::SqlModal);
            state.sql_modal.content = query.to_string();
            state.session.dsn = Some("postgres://localhost/test".to_string());
            state
        }

        #[test]
        fn submit_select_executes_immediately() {
            let mut state = modal_state_with_query("SELECT 1");

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now()).unwrap();

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ExecuteAdhoc { .. }))
            );
        }

        #[test]
        fn submit_insert_enters_confirming_low_risk() {
            let mut state = modal_state_with_query("INSERT INTO t VALUES (1)");

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now()).unwrap();

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::Confirming(d) if d.risk_level == RiskLevel::Low
            ));
            assert!(effects.is_empty());
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

        #[test]
        fn confirm_execute_transitions_to_running_and_emits_effect() {
            let mut state = modal_state_with_query("INSERT INTO t VALUES (1)");
            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalConfirmExecute, Instant::now())
                    .unwrap();

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ExecuteAdhoc { .. }))
            );
        }

        #[test]
        fn cancel_confirm_returns_to_editing() {
            let mut state = modal_state_with_query("INSERT INTO t VALUES (1)");
            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            reduce_sql_modal(&mut state, &Action::SqlModalCancelConfirm, Instant::now());

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Editing);
        }

        #[test]
        fn confirm_execute_in_editing_state_is_noop() {
            let mut state = modal_state_with_query("SELECT 1");

            let result =
                reduce_sql_modal(&mut state, &Action::SqlModalConfirmExecute, Instant::now());

            assert!(result.is_none());
        }
    }
}
