mod execution;
mod pagination;
mod write;

use std::time::Instant;

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::services::AppServices;
use crate::app::state::AppState;

pub fn reduce_query(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> Option<Vec<Effect>> {
    execution::reduce(state, action, now, services)
        .or_else(|| write::reduce(state, action, now, services))
        .or_else(|| pagination::reduce(state, action, now, services))
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use crate::app::state::AppState;
    use crate::domain::{
        Column, CommandTag, Index, IndexType, QueryResult, QuerySource, Table, Trigger,
        TriggerEvent, TriggerTiming,
    };

    pub fn create_test_state() -> AppState {
        let mut state = AppState::new("test_project".to_string());
        state.session.dsn = Some("postgres://localhost/test".to_string());
        state
    }

    pub fn preview_result(row_count: usize) -> Arc<QueryResult> {
        let rows: Vec<Vec<String>> = (0..row_count).map(|i| vec![i.to_string()]).collect();
        Arc::new(QueryResult {
            query: "SELECT * FROM users".to_string(),
            columns: vec!["id".to_string()],
            rows,
            row_count,
            execution_time_ms: 10,
            executed_at: Instant::now(),
            source: QuerySource::Preview,
            error: None,
            command_tag: None,
        })
    }

    pub fn adhoc_result() -> Arc<QueryResult> {
        Arc::new(QueryResult {
            query: "SELECT 1".to_string(),
            columns: vec!["id".to_string()],
            rows: vec![vec!["1".to_string()]],
            row_count: 1,
            execution_time_ms: 10,
            executed_at: Instant::now(),
            source: QuerySource::Adhoc,
            error: None,
            command_tag: None,
        })
    }

    pub fn editable_preview_result() -> Arc<QueryResult> {
        Arc::new(QueryResult {
            query: "SELECT * FROM users".to_string(),
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![vec!["1".to_string(), "Alice".to_string()]],
            row_count: 1,
            execution_time_ms: 10,
            executed_at: Instant::now(),
            source: QuerySource::Preview,
            error: None,
            command_tag: None,
        })
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
                    nullable: false,
                    default: None,
                    is_primary_key: true,
                    is_unique: true,
                    comment: None,
                    ordinal_position: 1,
                },
                Column {
                    name: "name".to_string(),
                    data_type: "text".to_string(),
                    nullable: true,
                    default: None,
                    is_primary_key: false,
                    is_unique: false,
                    comment: None,
                    ordinal_position: 2,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            indexes: vec![Index {
                name: "users_pkey".to_string(),
                columns: vec!["id".to_string()],
                is_unique: true,
                is_primary: true,
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

    pub fn adhoc_result_with_tag(tag: CommandTag) -> Arc<QueryResult> {
        Arc::new(QueryResult {
            query: String::new(),
            columns: vec![],
            rows: vec![],
            row_count: 0,
            execution_time_ms: 5,
            executed_at: Instant::now(),
            source: QuerySource::Adhoc,
            error: None,
            command_tag: Some(tag),
        })
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
        state.query.pagination.schema = schema.to_string();
        state.query.pagination.table = table.to_string();
        state
    }
}
