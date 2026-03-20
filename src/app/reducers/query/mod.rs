mod execution;
mod pagination;
mod write;

use std::time::Instant;

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::query_execution::PREVIEW_PAGE_SIZE;
use crate::app::services::AppServices;
use crate::app::state::AppState;
use crate::app::write_guardrails::{
    ColumnDiff, RiskLevel, WriteOperation, WritePreview, evaluate_guardrails,
};
use crate::app::write_update::{build_pk_pairs, escape_preview_value};
use crate::domain::{QueryResult, QuerySource};

use super::helpers::editable_preview_base;

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

pub(super) fn build_update_preview(
    state: &AppState,
    services: &AppServices,
) -> Result<WritePreview, String> {
    if !state.result_interaction.cell_edit().is_active() {
        return Err("No active cell edit session".to_string());
    }

    let (result, pk_cols) = editable_preview_base(state)?;

    let row_idx = state
        .result_interaction
        .cell_edit()
        .row
        .ok_or_else(|| "No row selected for edit".to_string())?;
    let col_idx = state
        .result_interaction
        .cell_edit()
        .col
        .ok_or_else(|| "No column selected for edit".to_string())?;

    let row = result
        .rows
        .get(row_idx)
        .ok_or_else(|| "Row index out of bounds".to_string())?;
    let column_name = result
        .columns
        .get(col_idx)
        .ok_or_else(|| "Column index out of bounds".to_string())?
        .clone();

    if pk_cols.iter().any(|pk| pk == &column_name) {
        return Err("Primary key columns are read-only".to_string());
    }

    let pk_pairs = build_pk_pairs(&result.columns, row, pk_cols);
    let target = crate::app::write_guardrails::TargetSummary {
        schema: state.query.pagination.schema.clone(),
        table: state.query.pagination.table.clone(),
        key_values: pk_pairs.clone().unwrap_or_default(),
    };
    let has_where = pk_pairs.as_ref().is_some_and(|pairs| !pairs.is_empty());
    let has_stable_row_identity = pk_pairs.is_some();
    let guardrail = evaluate_guardrails(has_where, has_stable_row_identity, Some(target.clone()));
    if guardrail.blocked {
        let reason = guardrail
            .reason
            .clone()
            .unwrap_or_else(|| "Write blocked by guardrails".to_string());
        return Err(reason);
    }

    let sql = services.sql_dialect.build_update_sql(
        &target.schema,
        &target.table,
        &column_name,
        state.result_interaction.cell_edit().draft_value(),
        &target.key_values,
    );
    let preview = WritePreview {
        operation: WriteOperation::Update,
        sql,
        target_summary: target,
        diff: vec![ColumnDiff {
            column: column_name,
            before: state.result_interaction.cell_edit().original_value.clone(),
            after: state
                .result_interaction
                .cell_edit()
                .draft_value()
                .to_string(),
        }],
        guardrail,
    };
    Ok(preview)
}

pub(super) fn build_write_preview_fallback_message(preview: &WritePreview) -> String {
    let mut lines = Vec::new();
    if preview.guardrail.risk_level != RiskLevel::Low {
        lines.push(format!("Risk: {}", preview.guardrail.risk_level.as_str()));
    }
    match preview.operation {
        WriteOperation::Update => {
            lines.push(preview.diff.first().map_or_else(
                || "(no changes)".to_string(),
                |d| {
                    format!(
                        "{}: \"{}\" -> \"{}\"",
                        d.column,
                        escape_preview_value(&d.before),
                        escape_preview_value(&d.after)
                    )
                },
            ));
        }
        WriteOperation::Delete => {
            lines.push(format!(
                "Target: {}",
                preview.target_summary.format_compact()
            ));
        }
    }
    lines.join("\n")
}

pub(super) fn try_adhoc_refresh(state: &mut AppState, result: &QueryResult) -> Vec<Effect> {
    if result.source != QuerySource::Adhoc || result.is_error() {
        return vec![];
    }
    let Some(tag) = &result.command_tag else {
        return vec![];
    };
    if !tag.needs_refresh() {
        return vec![];
    }
    let Some(dsn) = state.session.dsn.clone() else {
        return vec![];
    };

    let mut effects = vec![];

    if tag.is_schema_modifying() {
        state.sql_modal.reset_prefetch();
        state.session.set_table_detail_raw(None);

        effects.push(Effect::CacheInvalidate { dsn: dsn.clone() });
        effects.push(Effect::ClearCompletionEngineCache);
        effects.push(Effect::FetchMetadata { dsn });
    } else if !state.query.pagination.table.is_empty() {
        let page = state.query.pagination.current_page;
        effects.push(Effect::ExecutePreview {
            dsn,
            schema: state.query.pagination.schema.clone(),
            table: state.query.pagination.table.clone(),
            generation: state.session.selection_generation(),
            limit: PREVIEW_PAGE_SIZE,
            offset: page * PREVIEW_PAGE_SIZE,
            target_page: page,
            read_only: state.session.read_only,
        });
    }

    effects
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
