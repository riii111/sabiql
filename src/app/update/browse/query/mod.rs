mod execution;
mod pagination;
mod write;

use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::services::AppServices;
use crate::update::action::Action;

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

    use crate::domain::{
        Column, ColumnAttributes, CommandTag, Index, IndexAttributes, IndexType, QueryResult,
        QuerySource, Table, Trigger, TriggerEvent, TriggerTiming,
    };
    use crate::model::app_state::AppState;

    pub fn create_test_state() -> AppState {
        let mut state = AppState::new("test_project".to_string());
        state.session.set_dsn_for_test("postgres://localhost/test");
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
        Arc::new(QueryResult {
            query: "SELECT * FROM users".to_string(),
            columns: vec!["id".to_string(), "name".to_string(), "metadata".to_string()],
            rows: vec![vec![
                "1".to_string(),
                "Alice".to_string(),
                r#"{"role":"admin"}"#.to_string(),
            ]],
            row_count: 1,
            execution_time_ms: 10,
            executed_at: Instant::now(),
            source: QuerySource::Preview,
            error: None,
            command_tag: None,
        })
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
