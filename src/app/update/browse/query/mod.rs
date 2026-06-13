mod execution;
mod pagination;
mod write;

use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::browse::query_execution::PREVIEW_PAGE_SIZE;
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub fn dispatch_query(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> DispatchResult {
    execution::reduce_execution(state, action, now, services)
        .or_else(|| write::reduce_write(state, action, now, services))
        .or_else(|| pagination::reduce_pagination(state, action, now, services))
}

/// Builds the preview effect for the table currently held in pagination state,
/// issuing a fresh run_id. Returns `None` when no connection is active.
///
/// `generation` is the selection snapshot the eventual completion is validated
/// against. Refreshes of the active selection pass
/// `state.session.selection_generation()`; `Action::ExecutePreview` instead
/// passes the generation captured at selection time, so that results for a
/// selection cleared in the meantime (e.g. DROP TABLE + reload) are rejected.
pub(super) fn preview_effect_for_current_table(
    state: &mut AppState,
    now: Instant,
    target_page: usize,
    generation: u64,
) -> Option<Effect> {
    let dsn = state.session.dsn().map(String::from)?;
    let run_id = state.query.begin_running(now);
    Some(Effect::ExecutePreview {
        dsn,
        schema: state.query.pagination.schema().to_string(),
        table: state.query.pagination.table().to_string(),
        generation,
        run_id,
        limit: PREVIEW_PAGE_SIZE,
        offset: target_page * PREVIEW_PAGE_SIZE,
        target_page,
        read_only: state.session.is_read_only(),
    })
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use crate::domain::{
        Column, ColumnAttributes, CommandTag, Index, IndexAttributes, IndexType, QueryResult,
        QuerySource, Table, Trigger, TriggerEvent, TriggerTiming,
    };
    use crate::model::app_state::AppState;
    use crate::update::action::Action;
    use crate::update::test_support::activate_postgres_connection;

    pub fn create_test_state() -> AppState {
        let mut state = AppState::new("test_project".to_string());
        activate_postgres_connection(&mut state, "postgres://localhost/test");
        state
    }

    pub fn begin_query_run(state: &mut AppState) -> u64 {
        state.query.begin_running(Instant::now())
    }

    pub fn query_completed_action(
        state: &mut AppState,
        result: Arc<QueryResult>,
        generation: u64,
        target_page: Option<usize>,
    ) -> Action {
        let run_id = begin_query_run(state);
        Action::QueryCompleted {
            dsn: "postgres://localhost/test".to_string(),
            run_id,
            result,
            generation,
            target_page,
        }
    }

    pub fn preview_result(row_count: usize) -> Arc<QueryResult> {
        let rows: Vec<Vec<String>> = (0..row_count).map(|i| vec![i.to_string()]).collect();
        Arc::new(QueryResult::success(
            "SELECT * FROM users".to_string(),
            vec!["id".to_string()],
            rows,
            10,
            QuerySource::Preview,
        ))
    }

    pub fn adhoc_result() -> Arc<QueryResult> {
        Arc::new(QueryResult::success(
            "SELECT 1".to_string(),
            vec!["id".to_string()],
            vec![vec!["1".to_string()]],
            10,
            QuerySource::Adhoc,
        ))
    }

    pub fn editable_preview_result() -> Arc<QueryResult> {
        Arc::new(QueryResult::success(
            "SELECT * FROM users".to_string(),
            vec!["id".to_string(), "name".to_string()],
            vec![vec!["1".to_string(), "Alice".to_string()]],
            10,
            QuerySource::Preview,
        ))
    }

    pub fn users_table_detail() -> Table {
        Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            owner: None,
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "int".to_string(),
                    default: None,
                    attributes: ColumnAttributes::PRIMARY_KEY | ColumnAttributes::UNIQUE,
                    comment: None,
                    ordinal_position: 1,
                },
                Column {
                    name: "name".to_string(),
                    data_type: "text".to_string(),
                    default: None,
                    attributes: ColumnAttributes::NULLABLE,
                    comment: None,
                    ordinal_position: 2,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            indexes: vec![Index {
                name: "users_pkey".to_string(),
                columns: vec!["id".to_string()],
                attributes: IndexAttributes::UNIQUE | IndexAttributes::PRIMARY,
                index_type: IndexType::BTree,
                definition: None,
            }],
            rls: None,
            triggers: vec![Trigger {
                name: "trg".to_string(),
                timing: TriggerTiming::After,
                events: vec![TriggerEvent::Update],
                function_name: "f".to_string(),
                security_definer: false,
            }],
            row_count_estimate: None,
            comment: None,
        }
    }

    pub fn jsonb_table_detail() -> Table {
        let mut detail = users_table_detail();
        detail.columns.push(Column {
            name: "metadata".to_string(),
            data_type: "jsonb".to_string(),
            default: None,
            attributes: ColumnAttributes::NULLABLE,
            comment: None,
            ordinal_position: 3,
        });
        detail
    }

    pub fn editable_preview_result_with_jsonb() -> Arc<QueryResult> {
        Arc::new(QueryResult::success(
            "SELECT * FROM users".to_string(),
            vec!["id".to_string(), "name".to_string(), "metadata".to_string()],
            vec![vec![
                "1".to_string(),
                "Alice".to_string(),
                r#"{"role":"admin"}"#.to_string(),
            ]],
            10,
            QuerySource::Preview,
        ))
    }

    // Mirrors the executor's command-tag path: affected rows become row_count
    pub fn adhoc_result_with_tag(tag: CommandTag) -> Arc<QueryResult> {
        let mut result = QueryResult::success(String::new(), vec![], vec![], 5, QuerySource::Adhoc);
        result.row_count = tag.affected_rows().unwrap_or(0) as usize;
        Arc::new(result.with_command_tag(tag))
    }

    pub fn adhoc_error_result() -> Arc<QueryResult> {
        Arc::new(QueryResult::error(
            "BAD SQL".to_string(),
            "syntax error".to_string(),
            5,
            QuerySource::Adhoc,
        ))
    }

    pub fn state_with_table(schema: &str, table: &str) -> AppState {
        let mut state = create_test_state();
        state.query.pagination.reset_for_table(schema, table);
        state
    }
}
