pub(in crate::update) mod execution;
pub(in crate::update) mod pagination;
pub(in crate::update) mod write;

use std::sync::Arc;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::connection::ConnectionEndpoint;
use crate::model::app_state::AppState;
use crate::model::browse::query_execution::PREVIEW_PAGE_SIZE;
use crate::model::browse::session::TableDetailState;
use crate::update::helpers::reject_pending_mysql_connection_probe;

fn begin_preview_run(state: &mut AppState, now: Instant) -> Option<(Arc<ConnectionEndpoint>, u64)> {
    if reject_pending_mysql_connection_probe(state) {
        return None;
    }
    let endpoint = state.session.endpoint().cloned()?;
    Some((endpoint, state.query.begin_running(now)))
}

/// Builds the preview effect for the table currently held in pagination state,
/// issuing a fresh run_id. Returns `None` when no connection is active.
///
/// `generation` is the selection snapshot the eventual completion is validated against.
pub(super) fn preview_effect_for_current_table(
    state: &mut AppState,
    now: Instant,
    target_page: usize,
    generation: u64,
) -> Option<Effect> {
    let (endpoint, run_id) = begin_preview_run(state, now)?;
    // A preview shown before the selected table's detail settles would be read without its columns, so the detail is fetched again in the same ordered task.
    if generation == state.session.selection_generation()
        && state.session.selected_table_key().is_some()
        && matches!(
            state.session.table_detail_state(),
            TableDetailState::Loading
        )
    {
        return Some(Effect::LoadTable {
            endpoint,
            schema: state.query.pagination.schema().to_string(),
            table: state.query.pagination.table().to_string(),
            generation,
            detail_run_id: state.session.begin_table_detail_run(),
            preview_run_id: run_id,
            limit: PREVIEW_PAGE_SIZE,
            offset: target_page * PREVIEW_PAGE_SIZE,
            target_page,
        });
    }
    Some(Effect::ExecutePreview {
        endpoint,
        schema: state.query.pagination.schema().to_string(),
        table: state.query.pagination.table().to_string(),
        generation,
        run_id,
        limit: PREVIEW_PAGE_SIZE,
        offset: target_page * PREVIEW_PAGE_SIZE,
        target_page,
    })
}

pub(in crate::update) fn table_load_effect(
    state: &mut AppState,
    endpoint: Arc<ConnectionEndpoint>,
    now: Instant,
) -> Effect {
    let generation = state.session.selection_generation();
    let detail_run_id = state.session.begin_table_detail_run();
    let schema = state.query.pagination.schema().to_string();
    let table = state.query.pagination.table().to_string();
    let target_page = state.query.pagination.current_page();
    match begin_preview_run(state, now) {
        Some((_, preview_run_id)) => Effect::LoadTable {
            endpoint,
            schema,
            table,
            generation,
            detail_run_id,
            preview_run_id,
            limit: PREVIEW_PAGE_SIZE,
            offset: target_page * PREVIEW_PAGE_SIZE,
            target_page,
        },
        // A pending MySQL probe rejects previews, so only the detail is reloaded.
        None => Effect::FetchTableDetail {
            endpoint,
            schema,
            table,
            generation,
            run_id: detail_run_id,
        },
    }
}

#[cfg(test)]
pub(super) mod tests {
    use crate::test_support;

    use crate::domain::Column;
    use crate::domain::connection::ConnectionEndpoint;
    use std::sync::Arc;
    use std::time::Instant;

    use crate::domain::{
        ColumnAttributes, CommandTag, Index, IndexAttributes, IndexType, QueryResult, QuerySource,
        Table, Trigger,
    };
    use crate::model::app_state::AppState;
    use crate::update::action::{Action, QueryContext};
    use crate::update::test_fixtures;

    pub fn create_test_state() -> AppState {
        let mut state = AppState::new("test_project".to_string());
        test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/test");
        state
    }

    pub fn active_endpoint(state: &AppState) -> Arc<ConnectionEndpoint> {
        state
            .session
            .endpoint()
            .cloned()
            .expect("query test state must have an active connection")
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
            run_id,
            context: match target_page {
                Some(target_page) => QueryContext::Preview {
                    generation,
                    target_page,
                },
                None => QueryContext::Adhoc,
            },
            outcome: Ok(result),
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
            columns: vec![
                Column {
                    attributes: ColumnAttributes::PRIMARY_KEY | ColumnAttributes::UNIQUE,
                    ..test_support::column::test_nullable_column("id", "int", 1)
                },
                test_support::column::test_nullable_column("name", "text", 2),
            ],
            primary_key: Some(vec!["id".to_string()]),
            indexes: vec![Index {
                name: "users_pkey".to_string(),
                columns: vec!["id".to_string()],
                attributes: IndexAttributes::UNIQUE | IndexAttributes::PRIMARY,
                index_type: IndexType::BTree,
                definition: None,
            }],
            triggers: vec![Trigger {
                name: "trg".to_string(),
                definition: "CREATE TRIGGER trg AFTER UPDATE ON users EXECUTE FUNCTION f()"
                    .to_string(),
            }],
            ..test_support::table::minimal("", "")
        }
    }

    pub fn json_table_detail() -> Table {
        let mut detail = users_table_detail();
        detail
            .columns
            .push(test_support::column::test_nullable_column(
                "metadata", "jsonb", 3,
            ));
        detail
    }

    pub fn editable_preview_result_with_json() -> Arc<QueryResult> {
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
        Arc::new(
            QueryResult::success(String::new(), vec![], vec![], 5, QuerySource::Adhoc)
                .with_row_count(tag.affected_rows().unwrap_or(0) as usize)
                .with_command_tag(tag),
        )
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
