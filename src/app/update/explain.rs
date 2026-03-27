use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::shared::text_input::TextInputState;
use crate::app::model::sql_editor::completion::CompletionState;
use crate::app::model::sql_editor::modal::{SqlModalStatus, SqlModalTab};
use crate::app::policy::sql::statement_classifier;
use crate::app::policy::write::sql_risk::{ConfirmationType, evaluate_sql_risk, split_statements};
use crate::app::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};

fn is_multi_statement(content: &str) -> bool {
    split_statements(content).len() > 1
}

pub fn reduce_explain(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::ExplainRequest => {
            let content = state.sql_modal.editor.content().trim().to_string();
            if content.is_empty() {
                return Some(vec![]);
            }
            let Some(dsn) = &state.session.dsn else {
                return Some(vec![]);
            };
            if matches!(state.sql_modal.status(), SqlModalStatus::Running) {
                return Some(vec![]);
            }
            if is_multi_statement(&content) {
                state
                    .explain
                    .set_error("EXPLAIN does not support multiple statements".to_string());
                state.sql_modal.active_tab = SqlModalTab::Plan;
                return Some(vec![]);
            }

            let query = format!("EXPLAIN {content}");
            state.sql_modal.set_status(SqlModalStatus::Running);
            state.sql_modal.active_tab = SqlModalTab::Plan;
            state.explain.reset();
            state.query.begin_running(now);

            Some(vec![Effect::ExecuteExplain {
                dsn: dsn.clone(),
                query,
                is_analyze: false,
                read_only: true,
            }])
        }

        Action::ExplainAnalyzeRequest => {
            let content = state.sql_modal.editor.content().trim().to_string();
            if content.is_empty() {
                return Some(vec![]);
            }
            let Some(dsn) = &state.session.dsn else {
                return Some(vec![]);
            };
            let dsn = dsn.clone();
            if matches!(state.sql_modal.status(), SqlModalStatus::Running) {
                return Some(vec![]);
            }
            if is_multi_statement(&content) {
                state
                    .explain
                    .set_error("EXPLAIN ANALYZE does not support multiple statements".to_string());
                state.sql_modal.active_tab = SqlModalTab::Plan;
                return Some(vec![]);
            }
            let kind = statement_classifier::classify(&content);
            let risk = evaluate_sql_risk(&kind, &content);

            let is_dml = !matches!(
                risk.as_ref().map(|r| &r.confirmation),
                Some(ConfirmationType::Immediate) | None
            );

            if state.session.read_only && is_dml {
                state.explain.set_error(
                    "Read-only mode: EXPLAIN ANALYZE is blocked for DML statements.".into(),
                );
                state.sql_modal.active_tab = SqlModalTab::Plan;
                return Some(vec![]);
            }

            state.explain.confirm_scroll_offset = 0;

            match risk.map(|r| r.confirmation) {
                Some(ConfirmationType::Immediate) => {
                    // SELECT/Transaction: no confirmation, execute immediately
                    let explain_query = format!("EXPLAIN ANALYZE {content}");
                    state.sql_modal.set_status(SqlModalStatus::Running);
                    state.sql_modal.active_tab = SqlModalTab::Plan;
                    state.explain.reset();
                    state.query.begin_running(now);
                    return Some(vec![Effect::ExecuteExplain {
                        dsn,
                        query: explain_query,
                        is_analyze: true,
                        read_only: state.session.read_only,
                    }]);
                }
                Some(ConfirmationType::TableNameInput { target }) => {
                    state
                        .sql_modal
                        .set_status(SqlModalStatus::ConfirmingAnalyzeHigh {
                            query: content,
                            input: TextInputState::default(),
                            target_name: Some(target),
                        });
                    state.sql_modal.active_tab = SqlModalTab::Plan;
                }
                Some(ConfirmationType::Enter) => {
                    state
                        .sql_modal
                        .set_status(SqlModalStatus::ConfirmingAnalyze {
                            query: content,
                            is_dml: true,
                        });
                    state.sql_modal.active_tab = SqlModalTab::Plan;
                }
                None => {
                    // Unknown statement type: block
                    state
                        .explain
                        .set_error("Cannot determine risk level for this statement.".into());
                    state.sql_modal.active_tab = SqlModalTab::Plan;
                }
            }

            Some(vec![])
        }

        Action::Scroll {
            target: ScrollTarget::ExplainConfirm,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        } => {
            state.explain.confirm_scroll_offset =
                state.explain.confirm_scroll_offset.saturating_sub(1);
            Some(vec![])
        }

        Action::Scroll {
            target: ScrollTarget::ExplainConfirm,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        } => {
            // blank + title + blank + separator + blank + warning(2) + blank = 8
            const CONFIRM_HEADER_LINES: usize = 8;
            let content_lines =
                CONFIRM_HEADER_LINES + state.sql_modal.editor.content().lines().count();
            let modal_inner =
                crate::app::model::explain_context::ExplainContext::modal_inner_height(
                    state.ui.terminal_height,
                );
            let max = content_lines.saturating_sub(modal_inner);
            if state.explain.confirm_scroll_offset < max {
                state.explain.confirm_scroll_offset += 1;
            }
            Some(vec![])
        }

        Action::ExplainAnalyzeConfirm => {
            let query = match state.sql_modal.status() {
                SqlModalStatus::ConfirmingAnalyze { query, .. } => Some(query.clone()),
                SqlModalStatus::ConfirmingAnalyzeHigh {
                    query,
                    input,
                    target_name,
                } => target_name
                    .as_ref()
                    .is_some_and(|name| input.content() == name)
                    .then(|| query.clone()),
                _ => None,
            };
            if let Some(query) = query
                && let Some(dsn) = &state.session.dsn
            {
                let explain_query = format!("EXPLAIN ANALYZE {query}");
                state.sql_modal.set_status(SqlModalStatus::Running);
                state.explain.reset();
                state.query.begin_running(now);
                return Some(vec![Effect::ExecuteExplain {
                    dsn: dsn.clone(),
                    query: explain_query,
                    is_analyze: true,
                    read_only: state.session.read_only,
                }]);
            }
            Some(vec![])
        }

        Action::ExplainAnalyzeCancel => {
            if matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingAnalyze { .. }
                    | SqlModalStatus::ConfirmingAnalyzeHigh { .. }
            ) {
                state.sql_modal.set_status(SqlModalStatus::Normal);
            }
            Some(vec![])
        }

        Action::ExplainCompleted {
            plan_text,
            is_analyze,
            execution_time_ms,
        } => {
            let query = state.sql_modal.editor.content().to_string();
            state
                .explain
                .set_plan(plan_text.clone(), *is_analyze, *execution_time_ms, &query);
            state.sql_modal.set_status(SqlModalStatus::Normal);
            state.sql_modal.active_tab = SqlModalTab::Plan;
            state.query.mark_idle();
            Some(vec![])
        }

        Action::ExplainFailed(error) => {
            state.explain.set_error(error.to_string());
            state.sql_modal.set_status(SqlModalStatus::Normal);
            state.sql_modal.active_tab = SqlModalTab::Plan;
            state.query.mark_idle();
            Some(vec![])
        }

        Action::Scroll {
            target: ScrollTarget::ExplainPlan,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        } => {
            state.explain.scroll_offset = state.explain.scroll_offset.saturating_sub(1);
            Some(vec![])
        }

        Action::Scroll {
            target: ScrollTarget::ExplainPlan,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        } => {
            let modal_inner =
                crate::app::model::explain_context::ExplainContext::modal_inner_height(
                    state.ui.terminal_height,
                );
            let max = state.explain.line_count().saturating_sub(modal_inner);
            if state.explain.scroll_offset < max {
                state.explain.scroll_offset += 1;
            }
            Some(vec![])
        }

        Action::Scroll {
            target: ScrollTarget::ExplainCompare,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        } => {
            state.explain.compare_scroll_offset =
                state.explain.compare_scroll_offset.saturating_sub(1);
            Some(vec![])
        }

        Action::Scroll {
            target: ScrollTarget::ExplainCompare,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        } => {
            let max = state.explain.compare_max_scroll(state.ui.terminal_height);
            if state.explain.compare_scroll_offset < max {
                state.explain.compare_scroll_offset += 1;
            }
            Some(vec![])
        }

        Action::CompareEditQuery => {
            if let Some(ref right) = state.explain.right {
                let query = right.full_query.clone();
                state.sql_modal.editor.set_content(query);
                state.sql_modal.set_status(SqlModalStatus::Editing);
                state.sql_modal.completion = CompletionState::default();
                state.sql_modal.active_tab = SqlModalTab::Sql;
            }
            Some(vec![])
        }

        Action::SqlModalNextTab => {
            state.sql_modal.active_tab = match state.sql_modal.active_tab {
                SqlModalTab::Sql => SqlModalTab::Plan,
                SqlModalTab::Plan => SqlModalTab::Compare,
                SqlModalTab::Compare => SqlModalTab::Sql,
            };
            Some(vec![])
        }

        Action::SqlModalPrevTab => {
            state.sql_modal.active_tab = match state.sql_modal.active_tab {
                SqlModalTab::Sql => SqlModalTab::Compare,
                SqlModalTab::Compare => SqlModalTab::Plan,
                SqlModalTab::Plan => SqlModalTab::Sql,
            };
            Some(vec![])
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::input_mode::InputMode;
    use std::time::Instant;

    fn sql_modal_state() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::SqlModal);
        state
    }

    mod explain_request {
        use super::*;

        #[test]
        fn empty_query_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content("  ".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            let effects =
                reduce_explain(&mut state, &Action::ExplainRequest, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn no_dsn_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content("SELECT 1".to_string());

            let effects =
                reduce_explain(&mut state, &Action::ExplainRequest, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn running_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content("SELECT 1".to_string());
            state.session.dsn = Some("dsn://test".to_string());
            state.sql_modal.set_status(SqlModalStatus::Running);

            let effects =
                reduce_explain(&mut state, &Action::ExplainRequest, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn multi_statement_sets_error_and_switches_to_plan_tab() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("SELECT 1; DELETE FROM users".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            let effects =
                reduce_explain(&mut state, &Action::ExplainRequest, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(
                state.explain.error.as_deref(),
                Some("EXPLAIN does not support multiple statements")
            );
            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Plan);
        }

        #[test]
        fn starts_query_timer() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content("SELECT 1".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            reduce_explain(&mut state, &Action::ExplainRequest, Instant::now());

            assert!(state.query.is_running());
            assert!(state.query.start_time().is_some());
        }

        #[test]
        fn emits_execute_explain_effect() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content("SELECT 1".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            let effects =
                reduce_explain(&mut state, &Action::ExplainRequest, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(
                &effects[0],
                Effect::ExecuteExplain {
                    query,
                    is_analyze: false,
                    read_only: true,
                    ..
                } if query == "EXPLAIN SELECT 1"
            ));
            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Plan);
        }
    }

    mod explain_analyze_request {
        use super::*;

        #[test]
        fn empty_query_is_noop() {
            let mut state = sql_modal_state();
            state.session.dsn = Some("dsn://test".to_string());

            let effects =
                reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn multi_statement_sets_error_and_switches_to_plan_tab() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("SELECT 1; DELETE FROM users".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            let effects =
                reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(
                state.explain.error.as_deref(),
                Some("EXPLAIN ANALYZE does not support multiple statements")
            );
            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Plan);
            assert!(state.confirm_dialog.intent().is_none());
        }

        #[test]
        fn select_executes_immediately_without_confirm() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content("SELECT 1".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            let effects =
                reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(
                &effects[0],
                Effect::ExecuteExplain {
                    is_analyze: true,
                    ..
                }
            ));
            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
        }

        #[test]
        fn insert_shows_enter_confirm() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("INSERT INTO users VALUES (1)".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingAnalyze { is_dml: true, .. }
            ));
        }

        #[test]
        fn update_with_where_shows_enter_confirm() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("UPDATE users SET name='x' WHERE id=1".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingAnalyze { is_dml: true, .. }
            ));
        }

        #[test]
        fn delete_without_where_shows_high_confirm() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("DELETE FROM users".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingAnalyzeHigh {
                    target_name: Some(_),
                    ..
                }
            ));
        }

        #[test]
        fn delete_with_where_shows_enter_confirm() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("DELETE FROM users WHERE id=1".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingAnalyze { is_dml: true, .. }
            ));
        }

        #[test]
        fn drop_shows_high_confirm() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("DROP TABLE users".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingAnalyzeHigh {
                    target_name: Some(_),
                    ..
                }
            ));
        }

        #[test]
        fn truncate_shows_high_confirm() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("TRUNCATE users".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingAnalyzeHigh {
                    target_name: Some(_),
                    ..
                }
            ));
        }
    }

    mod read_only_analyze {
        use super::*;

        #[test]
        fn read_only_blocks_dml_analyze() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("DELETE FROM users WHERE id=1".to_string());
            state.session.dsn = Some("dsn://test".to_string());
            state.session.read_only = true;

            reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now());

            assert!(state.explain.error.is_some());
            assert!(
                state
                    .explain
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("Read-only")
            );
            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Plan);
            assert!(state.confirm_dialog.intent().is_none());
        }

        #[test]
        fn read_only_allows_select_analyze() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("SELECT * FROM users".to_string());
            state.session.dsn = Some("dsn://test".to_string());
            state.session.read_only = true;

            let effects =
                reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now()).unwrap();

            assert!(state.explain.error.is_none());
            assert_eq!(effects.len(), 1);
            assert!(matches!(
                &effects[0],
                Effect::ExecuteExplain {
                    is_analyze: true,
                    ..
                }
            ));
        }

        #[test]
        fn read_only_blocks_insert_analyze() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("INSERT INTO users VALUES (1)".to_string());
            state.session.dsn = Some("dsn://test".to_string());
            state.session.read_only = true;

            reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now());

            assert!(
                state
                    .explain
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("Read-only")
            );
        }
    }

    mod analyze_confirm_cancel {
        use super::*;

        #[test]
        fn confirm_from_confirming_analyze_emits_effect() {
            let mut state = sql_modal_state();
            state.session.dsn = Some("dsn://test".to_string());
            state
                .sql_modal
                .set_status(SqlModalStatus::ConfirmingAnalyze {
                    query: "INSERT INTO users VALUES (1)".to_string(),
                    is_dml: true,
                });

            let effects =
                reduce_explain(&mut state, &Action::ExplainAnalyzeConfirm, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(
                &effects[0],
                Effect::ExecuteExplain {
                    is_analyze: true,
                    ..
                }
            ));
            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
        }

        #[test]
        fn confirm_from_high_with_matching_table_emits_effect() {
            let mut state = sql_modal_state();
            state.session.dsn = Some("dsn://test".to_string());
            let mut input = crate::app::model::shared::text_input::TextInputState::default();
            for c in "users".chars() {
                input.insert_char(c);
            }
            state
                .sql_modal
                .set_status(SqlModalStatus::ConfirmingAnalyzeHigh {
                    query: "DELETE FROM users".to_string(),
                    input,
                    target_name: Some("users".to_string()),
                });

            let effects =
                reduce_explain(&mut state, &Action::ExplainAnalyzeConfirm, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
        }

        #[test]
        fn confirm_from_high_with_mismatch_is_noop() {
            let mut state = sql_modal_state();
            state.session.dsn = Some("dsn://test".to_string());
            let mut input = crate::app::model::shared::text_input::TextInputState::default();
            input.insert_char('x');
            state
                .sql_modal
                .set_status(SqlModalStatus::ConfirmingAnalyzeHigh {
                    query: "DELETE FROM users".to_string(),
                    input,
                    target_name: Some("users".to_string()),
                });

            let effects =
                reduce_explain(&mut state, &Action::ExplainAnalyzeConfirm, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingAnalyzeHigh { .. }
            ));
        }

        #[test]
        fn cancel_resets_to_normal() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .set_status(SqlModalStatus::ConfirmingAnalyze {
                    query: "UPDATE users SET x=1".to_string(),
                    is_dml: true,
                });

            reduce_explain(&mut state, &Action::ExplainAnalyzeCancel, Instant::now());

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Normal);
        }

        #[test]
        fn cancel_from_high_resets_to_normal() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .set_status(SqlModalStatus::ConfirmingAnalyzeHigh {
                    query: "DROP TABLE users".to_string(),
                    input: TextInputState::default(),
                    target_name: Some("users".to_string()),
                });

            reduce_explain(&mut state, &Action::ExplainAnalyzeCancel, Instant::now());

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Normal);
        }
    }

    mod explain_completed {
        use super::*;

        #[test]
        fn sets_plan_and_switches_to_plan_tab() {
            let mut state = sql_modal_state();
            state.sql_modal.set_status(SqlModalStatus::Running);

            reduce_explain(
                &mut state,
                &Action::ExplainCompleted {
                    plan_text: "Seq Scan".to_string(),
                    is_analyze: false,
                    execution_time_ms: 42,
                },
                Instant::now(),
            );

            assert_eq!(state.explain.plan_text.as_deref(), Some("Seq Scan"));
            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Normal);
            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Plan);
            assert!(!state.query.is_running());
        }
    }

    mod explain_failed {
        use super::*;
        use crate::app::ports::DbOperationError;

        #[test]
        fn sets_error_and_switches_to_plan_tab() {
            let mut state = sql_modal_state();
            state.sql_modal.set_status(SqlModalStatus::Running);

            reduce_explain(
                &mut state,
                &Action::ExplainFailed(DbOperationError::QueryFailed("syntax error".to_string())),
                Instant::now(),
            );

            assert_eq!(
                state.explain.error.as_deref(),
                Some("Query failed: syntax error")
            );
            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Normal);
            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Plan);
            assert!(!state.query.is_running());
        }
    }

    mod compare_workflow {
        use super::*;

        #[test]
        fn two_explains_auto_advance_returns_comparable_slots() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content("SELECT 1".to_string());
            state.session.dsn = Some("dsn://test".to_string());
            let now = Instant::now();

            // Step 1: First EXPLAIN
            reduce_explain(&mut state, &Action::ExplainRequest, now);
            reduce_explain(
                &mut state,
                &Action::ExplainCompleted {
                    plan_text: "Seq Scan  (cost=0.00..100.00 rows=10 width=32)".to_string(),
                    is_analyze: false,
                    execution_time_ms: 42,
                },
                now,
            );
            assert!(state.explain.right.is_some());
            assert!(state.explain.left.is_none());

            // Step 2: Second EXPLAIN — auto-advance moves right→left
            state.sql_modal.editor.set_content("SELECT 2".to_string());
            reduce_explain(&mut state, &Action::ExplainRequest, now);
            reduce_explain(
                &mut state,
                &Action::ExplainCompleted {
                    plan_text: "Index Scan  (cost=0.00..5.00 rows=1 width=32)".to_string(),
                    is_analyze: false,
                    execution_time_ms: 5,
                },
                now,
            );

            assert!(state.explain.left.is_some());
            assert!(state.explain.right.is_some());
            assert_eq!(
                state.explain.left.as_ref().unwrap().plan.total_cost,
                Some(100.0)
            );
            assert_eq!(
                state.explain.right.as_ref().unwrap().plan.total_cost,
                Some(5.0)
            );
        }
    }

    mod scroll {
        use super::*;

        #[test]
        fn scroll_up_saturates_at_zero() {
            let mut state = sql_modal_state();
            state.explain.scroll_offset = 0;

            reduce_explain(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::ExplainPlan,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::Line,
                },
                Instant::now(),
            );

            assert_eq!(state.explain.scroll_offset, 0);
        }

        #[test]
        fn scroll_down_increments() {
            let mut state = sql_modal_state();
            state.ui.terminal_height = 24;
            let long_plan = (0..20)
                .map(|i| format!("line{i}"))
                .collect::<Vec<_>>()
                .join("\n");
            state.explain.set_plan(long_plan, false, 0, "Q1");

            reduce_explain(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::ExplainPlan,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line,
                },
                Instant::now(),
            );

            assert_eq!(state.explain.scroll_offset, 1);
        }

        #[test]
        fn scroll_down_clamps_at_max() {
            let mut state = sql_modal_state();
            state.ui.terminal_height = 24;
            let long_plan = (0..20)
                .map(|i| format!("line{i}"))
                .collect::<Vec<_>>()
                .join("\n");
            state.explain.set_plan(long_plan, false, 0, "Q1");
            let modal_inner =
                crate::app::model::explain_context::ExplainContext::modal_inner_height(
                    state.ui.terminal_height,
                );
            let max = state.explain.line_count().saturating_sub(modal_inner);
            state.explain.scroll_offset = max;

            reduce_explain(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::ExplainPlan,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line,
                },
                Instant::now(),
            );

            assert_eq!(state.explain.scroll_offset, max);
        }

        #[test]
        fn compare_scroll_up_saturates_at_zero() {
            let mut state = sql_modal_state();
            state.explain.compare_scroll_offset = 0;

            reduce_explain(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::ExplainCompare,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::Line,
                },
                Instant::now(),
            );

            assert_eq!(state.explain.compare_scroll_offset, 0);
        }

        #[test]
        fn compare_scroll_down_increments() {
            let mut state = sql_modal_state();
            let long_plan = (0..20)
                .map(|i| format!("  ->  Node{i}  (cost=0.00..{i}.00 rows=1 width=32)"))
                .collect::<Vec<_>>()
                .join("\n");
            state.explain.set_plan(long_plan.clone(), false, 0, "Q1");
            state.explain.set_plan(long_plan, false, 0, "Q2");

            reduce_explain(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::ExplainCompare,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line,
                },
                Instant::now(),
            );

            assert_eq!(state.explain.compare_scroll_offset, 1);
        }

        #[test]
        fn compare_scroll_down_stops_at_max() {
            let mut state = sql_modal_state();
            state.ui.terminal_height = 24;
            let long_plan = (0..20)
                .map(|i| format!("  ->  Node{i}  (cost=0.00..{i}.00 rows=1 width=32)"))
                .collect::<Vec<_>>()
                .join("\n");
            state.explain.set_plan(long_plan.clone(), false, 0, "Q1");
            state.explain.set_plan(long_plan, false, 0, "Q2");

            let max = state.explain.compare_max_scroll(state.ui.terminal_height);

            // Scroll to max
            for _ in 0..max + 5 {
                reduce_explain(
                    &mut state,
                    &Action::Scroll {
                        target: ScrollTarget::ExplainCompare,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::Line,
                    },
                    Instant::now(),
                );
            }

            assert_eq!(state.explain.compare_scroll_offset, max);

            // k should immediately scroll back
            reduce_explain(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::ExplainCompare,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::Line,
                },
                Instant::now(),
            );
            assert_eq!(state.explain.compare_scroll_offset, max.saturating_sub(1));
        }

        #[test]
        fn right_only_scroll_down_increments() {
            let mut state = sql_modal_state();
            state.ui.terminal_height = 24;
            let long_plan = (0..20)
                .map(|i| format!("  ->  Node{i}  (cost=0.00..{i}.00 rows=1 width=32)"))
                .collect::<Vec<_>>()
                .join("\n");
            state.explain.set_plan(long_plan, false, 0, "Q1");

            reduce_explain(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::ExplainCompare,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line,
                },
                Instant::now(),
            );

            assert_eq!(state.explain.compare_scroll_offset, 1);
        }

        #[test]
        fn compare_scroll_down_clamps_without_content() {
            let mut state = sql_modal_state();

            reduce_explain(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::ExplainCompare,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line,
                },
                Instant::now(),
            );

            assert_eq!(state.explain.compare_scroll_offset, 0);
        }
    }

    mod tab_switch {
        use super::*;

        #[test]
        fn next_tab_switches_sql_to_plan() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Sql;

            reduce_explain(&mut state, &Action::SqlModalNextTab, Instant::now());

            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Plan);
        }

        #[test]
        fn next_tab_switches_plan_to_compare() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Plan;

            reduce_explain(&mut state, &Action::SqlModalNextTab, Instant::now());

            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Compare);
        }

        #[test]
        fn next_tab_switches_compare_to_sql() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Compare;

            reduce_explain(&mut state, &Action::SqlModalNextTab, Instant::now());

            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Sql);
        }

        #[test]
        fn prev_tab_switches_sql_to_compare() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Sql;

            reduce_explain(&mut state, &Action::SqlModalPrevTab, Instant::now());

            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Compare);
        }

        #[test]
        fn prev_tab_switches_compare_to_plan() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Compare;

            reduce_explain(&mut state, &Action::SqlModalPrevTab, Instant::now());

            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Plan);
        }

        #[test]
        fn prev_tab_switches_plan_to_sql() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Plan;

            reduce_explain(&mut state, &Action::SqlModalPrevTab, Instant::now());

            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Sql);
        }
    }
}
