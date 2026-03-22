use std::time::{Duration, Instant};

use super::helpers::{char_count, char_to_byte_index};
use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::model::shared::text_input::TextInputState;
use crate::app::model::sql_editor::modal::{
    HIGH_RISK_INPUT_VISIBLE_WIDTH, SqlModalStatus, SqlModalTab,
};
use crate::app::policy::sql::statement_classifier::{self, StatementKind};
use crate::app::policy::write::sql_risk::{
    ConfirmationType, MultiStatementDecision, evaluate_multi_statement,
};
use crate::app::policy::write::write_guardrails::{
    AdhocRiskDecision, RiskLevel, evaluate_sql_risk,
};
use crate::app::update::action::{Action, CursorMove, InputTarget};
use crate::domain::explain_plan::{ComparisonVerdict, compare_plans};

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
            if !matches!(state.sql_modal.status(), SqlModalStatus::Editing) {
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
        Action::TextInput {
            target: InputTarget::SqlModal,
            ch: c,
        } => {
            state.sql_modal.set_status(SqlModalStatus::Editing);
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert(byte_idx, *c);
            state.sql_modal.cursor += 1;
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            Some(vec![])
        }
        Action::TextBackspace {
            target: InputTarget::SqlModal,
        } => {
            state.sql_modal.set_status(SqlModalStatus::Editing);
            if state.sql_modal.cursor > 0 {
                state.sql_modal.cursor -= 1;
                let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
                state.sql_modal.content.remove(byte_idx);
            }
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            Some(vec![])
        }
        Action::TextDelete {
            target: InputTarget::SqlModal,
        } => {
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
        Action::TextMoveCursor {
            target: InputTarget::SqlModal,
            direction: movement,
        } => {
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
            state.sql_modal.set_status(SqlModalStatus::Normal);
            state.sql_modal.active_tab = SqlModalTab::Sql;
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion.candidates.clear();
            state.sql_modal.completion.selected_index = 0;
            state.sql_modal.completion_debounce = None;
            state
                .flash_timers
                .clear(crate::app::model::shared::flash_timer::FlashId::SqlModal);
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
                MultiStatementDecision::Allow {
                    risk,
                    ref statements,
                } => {
                    let label = multi_statement_label(&query);
                    let decision = AdhocRiskDecision {
                        risk_level: risk.risk_level,
                        label,
                    };
                    // In read-only mode, block if any statement is a write operation
                    let has_write = statements.iter().any(|s| {
                        let kind = statement_classifier::classify(s);
                        !matches!(kind, StatementKind::Select | StatementKind::Transaction)
                    });
                    if state.session.read_only && has_write {
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
                state.sql_modal.set_status(SqlModalStatus::Normal);
                Some(vec![])
            } else {
                None
            }
        }

        // HIGH risk confirmation input
        Action::TextInput {
            target: InputTarget::SqlModalHighRisk,
            ch: c,
        } => {
            if let Some(input) = state.sql_modal.confirming_high_input_mut() {
                input.insert_char(*c);
                input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
            }
            Some(vec![])
        }
        Action::TextBackspace {
            target: InputTarget::SqlModalHighRisk,
        } => {
            if let Some(input) = state.sql_modal.confirming_high_input_mut() {
                input.backspace();
                input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
            }
            Some(vec![])
        }
        Action::TextMoveCursor {
            target: InputTarget::SqlModalHighRisk,
            direction: movement,
        } => {
            if let Some(input) = state.sql_modal.confirming_high_input_mut() {
                input.move_cursor(*movement);
                input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
            }
            Some(vec![])
        }
        // EXPLAIN ANALYZE high-risk confirmation input
        Action::TextInput {
            target: InputTarget::SqlModalAnalyzeHighRisk,
            ch: c,
        } => {
            if let Some(input) = state.sql_modal.confirming_analyze_high_input_mut() {
                input.insert_char(*c);
                input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
            }
            Some(vec![])
        }
        Action::TextBackspace {
            target: InputTarget::SqlModalAnalyzeHighRisk,
        } => {
            if let Some(input) = state.sql_modal.confirming_analyze_high_input_mut() {
                input.backspace();
                input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
            }
            Some(vec![])
        }
        Action::TextMoveCursor {
            target: InputTarget::SqlModalAnalyzeHighRisk,
            direction: movement,
        } => {
            if let Some(input) = state.sql_modal.confirming_analyze_high_input_mut() {
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

        Action::SqlModalEnterInsert => {
            state.sql_modal.set_status(SqlModalStatus::Editing);
            Some(vec![])
        }
        Action::SqlModalEnterNormal => {
            state.sql_modal.set_status(SqlModalStatus::Normal);
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion_debounce = None;
            Some(vec![])
        }
        Action::SqlModalYank => {
            let content = match state.sql_modal.active_tab {
                SqlModalTab::Plan => state.explain.plan_text.clone(),
                SqlModalTab::Compare => match (&state.explain.left, &state.explain.right) {
                    (Some(l), Some(r)) => {
                        let result = compare_plans(&l.plan, &r.plan);
                        let verdict = match result.verdict {
                            ComparisonVerdict::Improved => "Improved",
                            ComparisonVerdict::Worsened => "Worsened",
                            ComparisonVerdict::Similar => "Similar",
                            ComparisonVerdict::Unavailable => "Unavailable",
                        };
                        let mut verdict_section = verdict.to_string();
                        for reason in &result.reasons {
                            verdict_section.push_str(&format!("\n  • {}", reason));
                        }

                        let mut sections = vec![verdict_section];
                        for (pos, s) in [("Left", l), ("Right", r)] {
                            let mode = if s.plan.is_analyze {
                                "ANALYZE"
                            } else {
                                "EXPLAIN"
                            };
                            sections.push(format!(
                                "--- {}: {} ({}, {:.2}s) ---\n{}",
                                pos,
                                s.source.label(),
                                mode,
                                s.plan.execution_secs(),
                                s.plan.raw_text
                            ));
                        }
                        Some(sections.join("\n\n"))
                    }
                    _ => None,
                },
                SqlModalTab::Sql => {
                    if state.sql_modal.content.is_empty() {
                        None
                    } else {
                        Some(state.sql_modal.content.clone())
                    }
                }
            };
            match content {
                Some(c) if !c.is_empty() => Some(vec![Effect::CopyToClipboard {
                    content: c,
                    on_success: Some(Action::SqlModalYankSuccess),
                    on_failure: Some(Action::CopyFailed(crate::app::ports::ClipboardError {
                        message: "Clipboard unavailable".into(),
                    })),
                }]),
                _ => Some(vec![]),
            }
        }
        Action::SqlModalYankSuccess => {
            state.flash_timers.set(
                crate::app::model::shared::flash_timer::FlashId::SqlModal,
                now,
            );
            Some(vec![])
        }

        _ => None,
    }
}

fn multi_statement_label(sql: &str) -> &'static str {
    use crate::app::policy::write::sql_risk::split_statements;
    let mut worst_level = RiskLevel::Low;
    let mut worst_label = "SQL";
    for stmt in split_statements(sql) {
        let kind = statement_classifier::classify(&stmt);
        let d = evaluate_sql_risk(&kind);
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

    fn sql_modal_state() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::SqlModal);
        state
    }

    mod paste {
        use super::*;

        fn editing_state() -> AppState {
            let mut state = sql_modal_state();
            state.sql_modal.set_status(SqlModalStatus::Editing);
            state
        }

        #[test]
        fn paste_inserts_at_cursor() {
            let mut state = editing_state();
            state.sql_modal.content = "SELCT".to_string();
            state.sql_modal.cursor = 3;

            reduce_sql_modal(&mut state, &Action::Paste("E".to_string()), Instant::now());

            assert_eq!(state.sql_modal.content, "SELECT");
        }

        #[test]
        fn paste_preserves_newlines() {
            let mut state = editing_state();

            reduce_sql_modal(
                &mut state,
                &Action::Paste("SELECT\n*\nFROM".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.content, "SELECT\n*\nFROM");
        }

        #[test]
        fn paste_normalizes_crlf() {
            let mut state = editing_state();

            reduce_sql_modal(
                &mut state,
                &Action::Paste("a\r\nb".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.content, "a\nb");
        }

        #[test]
        fn paste_advances_cursor() {
            let mut state = editing_state();
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
            let mut state = editing_state();
            state.sql_modal.completion.visible = true;

            reduce_sql_modal(&mut state, &Action::Paste("x".to_string()), Instant::now());

            assert!(!state.sql_modal.completion.visible);
        }

        #[test]
        fn paste_with_multibyte() {
            let mut state = editing_state();
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
            let mut state = editing_state();
            state.sql_modal.content = "DROP TABLE users".to_string();
            state.sql_modal.set_status(SqlModalStatus::ConfirmingHigh {
                decision: crate::app::policy::write::write_guardrails::AdhocRiskDecision {
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
        use crate::app::policy::write::write_guardrails::AdhocRiskDecision;

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
                effects
                    .is_some_and(|e| e.iter().any(|ef| matches!(ef, Effect::ExecuteAdhoc { .. })))
            );
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
                crate::app::model::sql_editor::modal::AdhocSuccessSnapshot {
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
        use crate::app::policy::write::write_guardrails::RiskLevel;

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
        fn cancel_confirm_returns_to_normal() {
            let mut state = modal_state_with_query("INSERT INTO t VALUES (1)");
            reduce_sql_modal(&mut state, &Action::SqlModalSubmit, Instant::now());

            reduce_sql_modal(&mut state, &Action::SqlModalCancelConfirm, Instant::now());

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Normal);
        }

        #[test]
        fn confirm_execute_in_editing_state_is_noop() {
            let mut state = modal_state_with_query("SELECT 1");

            let result =
                reduce_sql_modal(&mut state, &Action::SqlModalConfirmExecute, Instant::now());

            assert!(result.is_none());
        }
    }

    mod normal_insert_mode {
        use super::*;

        #[test]
        fn enter_insert_transitions_to_editing() {
            let mut state = sql_modal_state();

            reduce_sql_modal(&mut state, &Action::SqlModalEnterInsert, Instant::now());

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Editing);
        }

        #[test]
        fn enter_normal_transitions_to_normal() {
            let mut state = sql_modal_state();
            state.sql_modal.set_status(SqlModalStatus::Editing);
            state.sql_modal.completion.visible = true;

            reduce_sql_modal(&mut state, &Action::SqlModalEnterNormal, Instant::now());

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Normal);
            assert!(!state.sql_modal.completion.visible);
        }

        #[test]
        fn yank_empty_content_is_noop() {
            let mut state = sql_modal_state();

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn yank_non_empty_emits_copy_effect() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "SELECT 1".to_string();

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

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

            assert!(state.flash_timers.is_active(
                crate::app::model::shared::flash_timer::FlashId::SqlModal,
                now
            ));
        }

        #[test]
        fn open_sql_modal_starts_in_normal() {
            let mut state = AppState::new("test".to_string());

            reduce_sql_modal(&mut state, &Action::OpenSqlModal, Instant::now());

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Normal);
        }

        #[test]
        fn open_sql_modal_resets_active_tab_to_sql() {
            let mut state = AppState::new("test".to_string());
            state.sql_modal.active_tab = SqlModalTab::Plan;

            reduce_sql_modal(&mut state, &Action::OpenSqlModal, Instant::now());

            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Sql);
        }

        #[test]
        fn paste_ignored_in_normal_mode() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "original".to_string();

            reduce_sql_modal(
                &mut state,
                &Action::Paste("injected".to_string()),
                Instant::now(),
            );

            assert_eq!(state.sql_modal.content, "original");
        }
    }

    mod yank {
        use super::*;
        use crate::app::model::explain_context::{CompareSlot, SlotSource};
        use crate::domain::explain_plan::ExplainPlan;

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
            state.sql_modal.content = "SELECT 1".to_string();
            state.sql_modal.active_tab = SqlModalTab::Sql;

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(
                matches!(&effects[0], Effect::CopyToClipboard { content, .. } if content == "SELECT 1")
            );
        }

        #[test]
        fn sql_tab_yank_empty_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.content = String::new();
            state.sql_modal.active_tab = SqlModalTab::Sql;

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn plan_tab_yank_copies_plan_text() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Plan;
            state.explain.plan_text = Some("Seq Scan on users".to_string());

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(
                matches!(&effects[0], Effect::CopyToClipboard { content, .. } if content == "Seq Scan on users")
            );
        }

        #[test]
        fn plan_tab_yank_no_plan_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Plan;
            state.explain.plan_text = None;

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn plan_tab_yank_error_state_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Plan;
            state.explain.plan_text = None;
            state.explain.error = Some("syntax error".to_string());

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn compare_tab_yank_both_slots() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Compare;
            state.explain.left = Some(make_slot("Seq Scan", false, 420, SlotSource::AutoPrevious));
            state.explain.right = Some(make_slot("Index Scan", true, 50, SlotSource::AutoLatest));

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

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
        fn compare_tab_yank_both_manual_distinguishable() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Compare;
            state.explain.left = Some(make_slot("Seq Scan", false, 300, SlotSource::Manual));
            state.explain.right = Some(make_slot("Index Scan", false, 100, SlotSource::Manual));

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            if let Effect::CopyToClipboard { content, .. } = &effects[0] {
                assert!(content.contains("--- Left: Manual"));
                assert!(content.contains("--- Right: Manual"));
            } else {
                panic!("expected CopyToClipboard");
            }
        }

        #[test]
        fn compare_tab_yank_right_only_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Compare;
            state.explain.left = None;
            state.explain.right = Some(make_slot("Index Scan", false, 100, SlotSource::AutoLatest));

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn compare_tab_yank_left_only_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Compare;
            state.explain.left = Some(make_slot("Seq Scan", false, 200, SlotSource::Pinned));
            state.explain.right = None;

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn compare_tab_yank_empty_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Compare;
            state.explain.left = None;
            state.explain.right = None;

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn compare_tab_yank_includes_verdict_with_reasons() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Compare;
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

            let effects =
                reduce_sql_modal(&mut state, &Action::SqlModalYank, Instant::now()).unwrap();

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
