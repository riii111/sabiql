use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::explain_context::ExplainContext;
use crate::app::model::shared::text_input::{TextInputLike, TextInputState};
use crate::app::model::sql_editor::completion::CompletionState;
use crate::app::model::sql_editor::modal::{SqlModalStatus, SqlModalTab};
use crate::app::policy::sql::statement_classifier::{self, StatementKind};
use crate::app::policy::write::sql_risk::{ConfirmationType, evaluate_sql_risk, split_statements};
use crate::app::services::AppServices;
use crate::app::update::action::{Action, ScrollAmount, ScrollTarget};

fn is_multi_statement(content: &str) -> bool {
    split_statements(content).len() > 1
}

fn reject_unsupported_explain(state: &mut AppState, services: &AppServices) -> bool {
    if services.db_capabilities.supports_explain {
        return false;
    }

    state
        .explain
        .set_error("EXPLAIN is unavailable for this database".to_string());
    state.sql_modal.active_tab = services
        .db_capabilities
        .normalize_sql_modal_tab(state.sql_modal.active_tab);
    true
}

pub fn reduce_explain_with_services(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> Option<Vec<Effect>> {
    match action {
        Action::ExplainRequest => {
            if reject_unsupported_explain(state, services) {
                return Some(vec![]);
            }
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

            let query = services.sql_dialect.build_explain_sql(&content);
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
            if reject_unsupported_explain(state, services) {
                return Some(vec![]);
            }
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

            let is_dml = !matches!(kind, StatementKind::Select | StatementKind::Transaction);

            if state.session.read_only && is_dml {
                state.explain.set_error(
                    "Read-only mode: EXPLAIN ANALYZE is blocked for DML statements.".into(),
                );
                state.sql_modal.active_tab = SqlModalTab::Plan;
                return Some(vec![]);
            }

            state.explain.confirm_scroll_offset = 0;

            match risk.confirmation {
                ConfirmationType::TableNameInput { target } => {
                    state
                        .sql_modal
                        .set_status(SqlModalStatus::ConfirmingAnalyzeHigh {
                            query: content,
                            input: TextInputState::default(),
                            target_name: Some(target),
                        });
                    state.sql_modal.active_tab = SqlModalTab::Plan;
                }
                ConfirmationType::Immediate => {
                    let explain_query = services.sql_dialect.build_explain_analyze_sql(&content);
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
            }

            Some(vec![])
        }

        Action::ExplainAnalyzeConfirm => {
            if reject_unsupported_explain(state, services) {
                return Some(vec![]);
            }
            let query = match state.sql_modal.status() {
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
                let explain_query = services.sql_dialect.build_explain_analyze_sql(&query);
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
                SqlModalStatus::ConfirmingAnalyzeHigh { .. }
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
            target:
                target @ (ScrollTarget::ExplainConfirm
                | ScrollTarget::ExplainPlan
                | ScrollTarget::ExplainCompare),
            direction,
            amount: ScrollAmount::Line,
        } => {
            let (offset, max) = match target {
                ScrollTarget::ExplainConfirm => {
                    // blank + title + blank + separator + blank + warning(2) + blank = 8
                    const CONFIRM_HEADER_LINES: usize = 8;
                    let content_lines =
                        CONFIRM_HEADER_LINES + state.sql_modal.editor.content().lines().count();
                    let modal_inner = ExplainContext::modal_inner_height(state.ui.terminal_height);
                    (
                        &mut state.explain.confirm_scroll_offset,
                        content_lines.saturating_sub(modal_inner),
                    )
                }
                ScrollTarget::ExplainPlan => {
                    let modal_inner = ExplainContext::modal_inner_height(state.ui.terminal_height);
                    let max = state.explain.line_count().saturating_sub(modal_inner);
                    (&mut state.explain.scroll_offset, max)
                }
                ScrollTarget::ExplainCompare => {
                    let max = state.explain.compare_max_scroll(state.ui.terminal_height);
                    (&mut state.explain.compare_scroll_offset, max)
                }
                _ => unreachable!(),
            };
            *offset = direction.clamp_vertical_offset(*offset, max, 1);
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
            state.sql_modal.active_tab = services
                .db_capabilities
                .next_sql_modal_tab(state.sql_modal.active_tab);
            Some(vec![])
        }

        Action::SqlModalPrevTab => {
            state.sql_modal.active_tab = services
                .db_capabilities
                .prev_sql_modal_tab(state.sql_modal.active_tab);
            Some(vec![])
        }

        _ => None,
    }
}

#[cfg(test)]
pub fn reduce_explain(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    reduce_explain_with_services(
        state,
        action,
        now,
        &crate::app::services::AppServices::stub(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::db_capabilities::DbCapabilities;
    use crate::app::model::shared::input_mode::InputMode;
    use crate::app::model::shared::inspector_tab::InspectorTab;
    use crate::app::ports::{DdlGenerator, SqlDialect};
    use crate::app::services::AppServices;
    use crate::app::update::action::ScrollDirection;
    use std::sync::Arc;
    use std::time::Instant;

    fn sql_modal_state() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::SqlModal);
        state
    }

    fn services_without_explain() -> AppServices {
        struct NoopDdl;
        impl DdlGenerator for NoopDdl {
            fn generate_ddl(&self, _table: &crate::domain::Table) -> String {
                String::new()
            }
        }

        struct NoopSql;
        impl SqlDialect for NoopSql {
            fn build_explain_sql(&self, query: &str) -> String {
                format!("EXPLAIN {query}")
            }

            fn build_explain_analyze_sql(&self, query: &str) -> String {
                format!("EXPLAIN ANALYZE {query}")
            }

            fn build_update_sql(
                &self,
                _schema: &str,
                _table: &str,
                _column: &str,
                _new_value: &str,
                _pk_pairs: &[(String, String)],
            ) -> String {
                String::new()
            }

            fn build_bulk_delete_sql(
                &self,
                _schema: &str,
                _table: &str,
                _pk_pairs_per_row: &[Vec<(String, String)>],
            ) -> String {
                String::new()
            }
        }

        AppServices {
            ddl_generator: Arc::new(NoopDdl),
            sql_dialect: Arc::new(NoopSql),
            db_capabilities: DbCapabilities::new_for_tests(false, false, vec![InspectorTab::Info]),
        }
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
        fn unsupported_database_sets_error_without_effects() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content("SELECT 1".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            let effects = reduce_explain_with_services(
                &mut state,
                &Action::ExplainRequest,
                Instant::now(),
                &services_without_explain(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(
                state.explain.error.as_deref(),
                Some("EXPLAIN is unavailable for this database")
            );
            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Sql);
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
        fn unsupported_database_sets_error_without_effects() {
            let mut state = sql_modal_state();
            state.sql_modal.editor.set_content("SELECT 1".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            let effects = reduce_explain_with_services(
                &mut state,
                &Action::ExplainAnalyzeRequest,
                Instant::now(),
                &services_without_explain(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(
                state.explain.error.as_deref(),
                Some("EXPLAIN is unavailable for this database")
            );
            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Sql);
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
        fn insert_executes_immediately() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("INSERT INTO users VALUES (1)".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            let effects =
                reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now()).unwrap();

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
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
        fn update_with_where_executes_immediately() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("UPDATE users SET name='x' WHERE id=1".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            let effects =
                reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now()).unwrap();

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
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
        fn delete_with_where_executes_immediately() {
            let mut state = sql_modal_state();
            state
                .sql_modal
                .editor
                .set_content("DELETE FROM users WHERE id=1".to_string());
            state.session.dsn = Some("dsn://test".to_string());

            let effects =
                reduce_explain(&mut state, &Action::ExplainAnalyzeRequest, Instant::now()).unwrap();

            assert_eq!(*state.sql_modal.status(), SqlModalStatus::Running);
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
        fn plan_scroll_up_saturates_at_zero() {
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
        fn plan_scroll_down_increments() {
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
        fn plan_scroll_down_clamps_at_max() {
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
        fn compare_plan_scroll_up_saturates_at_zero() {
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
        fn compare_plan_scroll_down_increments() {
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
        fn right_only_plan_scroll_down_increments() {
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
        fn next_tab_stays_on_sql_when_explain_is_unsupported() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Sql;

            reduce_explain_with_services(
                &mut state,
                &Action::SqlModalNextTab,
                Instant::now(),
                &services_without_explain(),
            );

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

        #[test]
        fn prev_tab_stays_on_sql_when_explain_is_unsupported() {
            let mut state = sql_modal_state();
            state.sql_modal.active_tab = SqlModalTab::Sql;

            reduce_explain_with_services(
                &mut state,
                &Action::SqlModalPrevTab,
                Instant::now(),
                &services_without_explain(),
            );

            assert_eq!(state.sql_modal.active_tab, SqlModalTab::Sql);
        }
    }
}
