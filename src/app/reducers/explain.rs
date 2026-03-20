use std::time::Instant;

use crate::app::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};
use crate::app::adhoc_risk::split_statements;
use crate::app::confirm_dialog_state::ConfirmIntent;
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::sql_modal_context::{SqlModalStatus, SqlModalTab};
use crate::app::state::AppState;
use crate::app::statement_classifier::{self, StatementKind};

fn is_multi_statement(content: &str) -> bool {
    split_statements(content).len() > 1
}

pub fn reduce_explain(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::ExplainRequest => {
            let content = state.sql_modal.content.trim().to_string();
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

            let query = format!("EXPLAIN {}", content);
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
            let content = state.sql_modal.content.trim().to_string();
            if content.is_empty() {
                return Some(vec![]);
            }
            if state.session.dsn.is_none() {
                return Some(vec![]);
            }
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
            let is_dml = matches!(
                kind,
                StatementKind::Insert
                    | StatementKind::Update { .. }
                    | StatementKind::Delete { .. }
                    | StatementKind::Drop
                    | StatementKind::Truncate
            );

            let message = if is_dml {
                "ANALYZE executes the query. DML side effects will occur."
            } else {
                "ANALYZE executes the query to collect actual statistics."
            };

            state.confirm_dialog.open(
                "EXPLAIN ANALYZE",
                message,
                ConfirmIntent::ExplainAnalyze {
                    query: content,
                    is_dml,
                },
            );
            state.modal.push_mode(InputMode::ConfirmDialog);

            Some(vec![])
        }

        Action::ExplainCompleted {
            plan_text,
            is_analyze,
            execution_time_ms,
        } => {
            state
                .explain
                .set_plan(plan_text.clone(), *is_analyze, *execution_time_ms);
            state.sql_modal.set_status(SqlModalStatus::Normal);
            state.sql_modal.active_tab = SqlModalTab::Plan;
            state.query.mark_idle();
            Some(vec![])
        }

        Action::ExplainFailed(error) => {
            state.explain.set_error(error.clone());
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
            let max = state.explain.line_count().saturating_sub(1);
            if state.explain.scroll_offset < max {
                state.explain.scroll_offset += 1;
            }
            Some(vec![])
        }

        Action::SqlModalNextTab => {
            state.sql_modal.active_tab = match state.sql_modal.active_tab {
                SqlModalTab::Sql => SqlModalTab::Plan,
                SqlModalTab::Plan => SqlModalTab::Sql,
            };
            Some(vec![])
        }

        Action::SqlModalPrevTab => {
            state.sql_modal.active_tab = match state.sql_modal.active_tab {
                SqlModalTab::Sql => SqlModalTab::Plan,
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
            state.sql_modal.content = "  ".to_string();
            state.session.dsn = Some("dsn://test".to_string());

            let effects =
                reduce_explain(&mut state, &Action::ExplainRequest, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn no_dsn_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "SELECT 1".to_string();

            let effects =
                reduce_explain(&mut state, &Action::ExplainRequest, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn running_is_noop() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "SELECT 1".to_string();
            state.session.dsn = Some("dsn://test".to_string());
            state.sql_modal.set_status(SqlModalStatus::Running);

            let effects =
                reduce_explain(&mut state, &Action::ExplainRequest, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn multi_statement_sets_error_and_switches_to_plan_tab() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "SELECT 1; DELETE FROM users".to_string();
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
            state.sql_modal.content = "SELECT 1".to_string();
            state.session.dsn = Some("dsn://test".to_string());

            reduce_explain(&mut state, &Action::ExplainRequest, Instant::now());

            assert!(state.query.is_running());
            assert!(state.query.start_time().is_some());
        }

        #[test]
        fn emits_execute_explain_effect() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "SELECT 1".to_string();
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
            state.sql_modal.content = "SELECT 1; DELETE FROM users".to_string();
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
        fn opens_confirm_dialog_for_select() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "SELECT 1".to_string();
            state.session.dsn = Some("dsn://test".to_string());

            reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now());

            assert!(matches!(
                state.confirm_dialog.intent(),
                Some(ConfirmIntent::ExplainAnalyze { is_dml: false, .. })
            ));
        }

        #[test]
        fn opens_confirm_dialog_for_dml() {
            let mut state = sql_modal_state();
            state.sql_modal.content = "DELETE FROM users WHERE id=1".to_string();
            state.session.dsn = Some("dsn://test".to_string());

            reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now());

            assert!(matches!(
                state.confirm_dialog.intent(),
                Some(ConfirmIntent::ExplainAnalyze { is_dml: true, .. })
            ));
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

        #[test]
        fn sets_error_and_switches_to_plan_tab() {
            let mut state = sql_modal_state();
            state.sql_modal.set_status(SqlModalStatus::Running);

            reduce_explain(
                &mut state,
                &Action::ExplainFailed("syntax error".to_string()),
                Instant::now(),
            );

            assert_eq!(state.explain.error.as_deref(), Some("syntax error"));
            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Normal);
            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Plan);
            assert!(!state.query.is_running());
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
            state
                .explain
                .set_plan("line1\nline2\nline3".to_string(), false, 0);

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
            state.explain.set_plan("line1\nline2".to_string(), false, 0);
            state.explain.scroll_offset = 1;

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
        fn next_tab_switches_plan_to_sql() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Plan;

            reduce_explain(&mut state, &Action::SqlModalNextTab, Instant::now());

            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Sql);
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
