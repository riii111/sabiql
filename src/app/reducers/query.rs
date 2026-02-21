//! Query sub-reducer: query execution and command line.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::action::Action;
use crate::app::command::{command_to_action, parse_command};
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::query_execution::{PREVIEW_PAGE_SIZE, QueryStatus};
use crate::app::sql_modal_context::SqlModalStatus;
use crate::app::state::AppState;
use crate::app::write_guardrails::{
    ColumnDiff, RiskLevel, WriteOperation, WritePreview, evaluate_guardrails,
};
use crate::app::write_update::{build_pk_pairs, build_update_sql};
use crate::domain::{QueryResult, QuerySource, Table};

fn build_update_preview(state: &AppState) -> Result<WritePreview, String> {
    if state.ui.input_mode != InputMode::CellEdit || !state.cell_edit.is_active() {
        return Err("Cell edit mode is not active".to_string());
    }

    if state.query.history_index.is_some() {
        return Err("Editing is unavailable while browsing history".to_string());
    }

    let result = state
        .query
        .current_result
        .as_ref()
        .ok_or_else(|| "No result to edit".to_string())?;
    if result.source != QuerySource::Preview || result.is_error() {
        return Err("Only Preview results are editable".to_string());
    }

    let row_idx = state
        .cell_edit
        .row
        .ok_or_else(|| "No row selected for edit".to_string())?;
    let col_idx = state
        .cell_edit
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

    let table_detail = state
        .cache
        .table_detail
        .as_ref()
        .ok_or_else(|| "Table metadata not loaded".to_string())?;
    let pk_cols = table_detail
        .primary_key
        .as_ref()
        .filter(|cols| !cols.is_empty())
        .ok_or_else(|| "Editing requires a PRIMARY KEY".to_string())?;

    if pk_cols.iter().any(|pk| pk == &column_name) {
        return Err("Primary key columns are read-only".to_string());
    }

    let target = crate::app::write_guardrails::TargetSummary {
        schema: state.query.pagination.schema.clone(),
        table: state.query.pagination.table.clone(),
        key_values: vec![],
    };
    let pk_pairs = build_pk_pairs(&result.columns, row, pk_cols);
    let has_where = pk_pairs.as_ref().is_some_and(|pairs| !pairs.is_empty());
    let has_stable_row_identity = pk_pairs.is_some();
    let mut target = target;
    if let Some(pairs) = &pk_pairs {
        target.key_values = pairs.clone();
    }
    let guardrail = evaluate_guardrails(has_where, has_stable_row_identity, Some(target.clone()));
    if guardrail.blocked {
        let reason = guardrail
            .reason
            .clone()
            .unwrap_or_else(|| "Write blocked by guardrails".to_string());
        return Err(reason);
    }

    let sql = build_update_sql(
        &target.schema,
        &target.table,
        &column_name,
        &state.cell_edit.draft_value,
        &target.key_values,
    );
    let preview = WritePreview {
        operation: WriteOperation::Update,
        sql,
        target_summary: target,
        diff: vec![ColumnDiff {
            column: column_name,
            before: state.cell_edit.original_value.clone(),
            after: state.cell_edit.draft_value.clone(),
        }],
        guardrail,
    };
    Ok(preview)
}

fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('\"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn escape_modal_diff_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\"', "\\\"")
        .replace('\n', "\\n")
}

fn is_simple_identifier(ident: &str) -> bool {
    let mut chars = ident.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        _ => return false,
    }

    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_sql_keyword(ident: &str) -> bool {
    matches!(
        ident.to_ascii_uppercase().as_str(),
        "ALL"
            | "ALTER"
            | "AND"
            | "ANY"
            | "AS"
            | "ASC"
            | "BY"
            | "CREATE"
            | "DELETE"
            | "DESC"
            | "DROP"
            | "FALSE"
            | "FROM"
            | "GROUP"
            | "IN"
            | "INSERT"
            | "INTO"
            | "IS"
            | "JOIN"
            | "KEY"
            | "LIMIT"
            | "NOT"
            | "NULL"
            | "ON"
            | "OR"
            | "ORDER"
            | "PRIMARY"
            | "SELECT"
            | "SET"
            | "TABLE"
            | "TRUE"
            | "UNION"
            | "UNIQUE"
            | "UPDATE"
            | "VALUES"
            | "WHERE"
    )
}

fn display_identifier(ident: &str) -> String {
    if is_simple_identifier(ident) && !is_sql_keyword(ident) {
        ident.to_string()
    } else {
        quote_ident(ident)
    }
}

fn is_numeric_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_lowercase();
    t.contains("int")
        || t.contains("numeric")
        || t.contains("decimal")
        || t.contains("real")
        || t.contains("double")
        || t.contains("serial")
}

fn is_boolean_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_lowercase();
    t.contains("bool")
}

fn is_numeric_literal(value: &str) -> bool {
    value.parse::<i128>().is_ok() || value.parse::<f64>().is_ok()
}

fn display_sql_value(value: &str, data_type: Option<&str>) -> String {
    if value == "NULL" {
        return "NULL".to_string();
    }

    if data_type.is_some_and(is_numeric_type) && is_numeric_literal(value) {
        return value.to_string();
    }

    if data_type.is_some_and(is_boolean_type) {
        let normalized = value.trim();
        if normalized.eq_ignore_ascii_case("true") || normalized.eq_ignore_ascii_case("t") {
            return "TRUE".to_string();
        }
        if normalized.eq_ignore_ascii_case("false") || normalized.eq_ignore_ascii_case("f") {
            return "FALSE".to_string();
        }
    }

    quote_literal(value)
}

fn column_data_type<'a>(table_detail: Option<&'a Table>, column: &str) -> Option<&'a str> {
    table_detail.and_then(|table| {
        table
            .columns
            .iter()
            .find(|c| c.name == column)
            .map(|c| c.data_type.as_str())
    })
}

fn format_update_sql_for_modal(preview: &WritePreview, table_detail: Option<&Table>) -> String {
    let Some(diff) = preview.diff.first() else {
        return preview.sql.clone();
    };

    if preview.target_summary.key_values.is_empty() {
        return preview.sql.clone();
    }

    let where_clause = preview
        .target_summary
        .key_values
        .iter()
        .map(|(col, val)| {
            let value = display_sql_value(val, column_data_type(table_detail, col));
            format!("{} = {}", display_identifier(col), value)
        })
        .collect::<Vec<_>>()
        .join(" AND ");

    let set_value = display_sql_value(&diff.after, column_data_type(table_detail, &diff.column));

    format!(
        "UPDATE {}.{}\nSET {} = {}\nWHERE {};",
        display_identifier(&preview.target_summary.schema),
        display_identifier(&preview.target_summary.table),
        display_identifier(&diff.column),
        set_value,
        where_clause
    )
}

/// Handles query execution and command line actions.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_query(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::QueryCompleted {
            result,
            generation,
            target_page,
        } => {
            if *generation == 0 || *generation == state.cache.selection_generation {
                state.query.status = QueryStatus::Idle;
                state.query.start_time = None;
                state.ui.result_scroll_offset = 0;
                state.ui.result_horizontal_offset = 0;
                state.ui.result_selection.reset();
                state.cell_edit.clear();
                state.pending_write_preview = None;
                state.query.result_highlight_until = Some(now + Duration::from_millis(500));
                state.query.history_index = None;

                if result.source == QuerySource::Adhoc {
                    if result.is_error() {
                        state.sql_modal.status = SqlModalStatus::Error;
                    } else {
                        state.sql_modal.status = SqlModalStatus::Success;
                    }
                }

                if result.source == QuerySource::Adhoc && !result.is_error() {
                    state.query.result_history.push(Arc::clone(result));
                }

                if let Some(page) = target_page {
                    state.query.pagination.current_page = *page;
                    if result.rows.len() < PREVIEW_PAGE_SIZE {
                        state.query.pagination.reached_end = true;
                    }
                }

                state.query.current_result = Some(Arc::clone(result));
            }
            Some(vec![])
        }
        Action::QueryFailed(error, generation) => {
            if *generation == 0 || *generation == state.cache.selection_generation {
                state.query.status = QueryStatus::Idle;
                state.query.start_time = None;
                state.ui.result_selection.reset();
                state.ui.result_scroll_offset = 0;
                state.ui.result_horizontal_offset = 0;
                state.cell_edit.clear();
                state.pending_write_preview = None;
                state.set_error(error.clone());
                if state.ui.input_mode == InputMode::SqlModal {
                    state.sql_modal.status = SqlModalStatus::Error;
                    let error_result = Arc::new(QueryResult::error(
                        state.sql_modal.content.clone(),
                        error.clone(),
                        0,
                        QuerySource::Adhoc,
                    ));
                    state.query.current_result = Some(error_result);
                }
            }
            Some(vec![])
        }

        Action::CommandLineSubmit => {
            let cmd = parse_command(&state.command_line_input);
            let follow_up = command_to_action(cmd);
            state.ui.input_mode = state.ui.command_line_return_mode;
            state.ui.command_line_return_mode = InputMode::Normal;
            state.command_line_input.clear();

            Some(match follow_up {
                Action::Quit => {
                    state.should_quit = true;
                    vec![]
                }
                Action::OpenHelp => {
                    state.ui.input_mode = InputMode::Help;
                    vec![]
                }
                Action::OpenSqlModal => {
                    state.ui.input_mode = InputMode::SqlModal;
                    state.sql_modal.status = SqlModalStatus::Editing;
                    if !state.sql_modal.prefetch_started && state.cache.metadata.is_some() {
                        vec![Effect::DispatchActions(vec![Action::StartPrefetchAll])]
                    } else {
                        vec![]
                    }
                }
                Action::ErOpenDiagram => {
                    vec![Effect::DispatchActions(vec![Action::ErOpenDiagram])]
                }
                Action::SubmitCellEditWrite => {
                    vec![Effect::DispatchActions(vec![Action::SubmitCellEditWrite])]
                }
                _ => vec![],
            })
        }

        Action::ExecutePreview {
            schema,
            table,
            generation,
        } => {
            if let Some(dsn) = &state.runtime.dsn {
                state.query.status = QueryStatus::Running;
                state.query.start_time = Some(now);

                // Initialize pagination for this preview
                state.query.pagination.reset();
                state.query.pagination.schema = schema.clone();
                state.query.pagination.table = table.clone();

                // Look up row_count_estimate from table detail or table summaries
                let row_estimate = state
                    .cache
                    .table_detail
                    .as_ref()
                    .and_then(|d| d.row_count_estimate)
                    .or_else(|| {
                        state.tables().iter().find_map(|t| {
                            if t.schema == *schema && t.name == *table {
                                t.row_count_estimate
                            } else {
                                None
                            }
                        })
                    });
                state.query.pagination.total_rows_estimate = row_estimate;

                Some(vec![Effect::ExecutePreview {
                    dsn: dsn.clone(),
                    schema: schema.clone(),
                    table: table.clone(),
                    generation: *generation,
                    limit: PREVIEW_PAGE_SIZE,
                    offset: 0,
                    target_page: 0,
                }])
            } else {
                Some(vec![])
            }
        }

        Action::ExecuteAdhoc(query) => {
            if let Some(dsn) = &state.runtime.dsn {
                state.query.status = QueryStatus::Running;
                state.query.start_time = Some(now);
                Some(vec![Effect::ExecuteAdhoc {
                    dsn: dsn.clone(),
                    query: query.clone(),
                }])
            } else {
                Some(vec![])
            }
        }

        Action::SubmitCellEditWrite => {
            if state.ui.input_mode != InputMode::CellEdit {
                state
                    .messages
                    .set_error_at("`:w` is only available in Cell Edit mode".to_string(), now);
                return Some(vec![]);
            }

            match build_update_preview(state) {
                Ok(preview) => Some(vec![Effect::DispatchActions(vec![
                    Action::OpenWritePreviewConfirm(Box::new(preview)),
                ])]),
                Err(msg) => {
                    state.messages.set_error_at(msg, now);
                    Some(vec![])
                }
            }
        }

        Action::OpenWritePreviewConfirm(preview) => {
            state.pending_write_preview = Some((**preview).clone());
            let mut lines = Vec::new();
            if preview.guardrail.risk_level != RiskLevel::Low {
                lines.push(format!("Risk: {}", preview.guardrail.risk_level.as_str()));
            }
            lines.push(preview.diff.first().map_or_else(
                || "(no changes)".to_string(),
                |d| {
                    format!(
                        "{}: \"{}\" -> \"{}\"",
                        d.column,
                        escape_modal_diff_value(&d.before),
                        escape_modal_diff_value(&d.after)
                    )
                },
            ));
            lines.push(String::new());
            lines.push(format_update_sql_for_modal(
                preview,
                state.cache.table_detail.as_ref(),
            ));
            let message = lines.join("\n");

            state.confirm_dialog.title =
                format!("Confirm UPDATE: {}", preview.target_summary.table);
            state.confirm_dialog.message = message;
            state.confirm_dialog.on_confirm = Action::ExecuteWrite(preview.sql.clone());
            state.confirm_dialog.on_cancel = Action::None;
            state.confirm_dialog.return_mode = InputMode::CellEdit;
            state.ui.input_mode = InputMode::ConfirmDialog;

            Some(vec![])
        }

        Action::ExecuteWrite(query) => {
            if let Some(dsn) = &state.runtime.dsn {
                state.query.status = QueryStatus::Running;
                state.query.start_time = Some(now);
                Some(vec![Effect::ExecuteWrite {
                    dsn: dsn.clone(),
                    query: query.clone(),
                }])
            } else {
                state
                    .messages
                    .set_error_at("No active connection".to_string(), now);
                Some(vec![])
            }
        }

        Action::ExecuteWriteSucceeded { affected_rows } => {
            state.query.status = QueryStatus::Idle;
            state.query.start_time = None;
            state.pending_write_preview = None;

            if *affected_rows != 1 {
                state.messages.set_error_at(
                    format!("UPDATE expected 1 row, but affected {} rows", affected_rows),
                    now,
                );
                state.ui.input_mode = InputMode::CellEdit;
                return Some(vec![]);
            }

            state
                .messages
                .set_success_at("Updated 1 row".to_string(), now);
            state.cell_edit.clear();
            state.ui.input_mode = InputMode::Normal;

            if let Some(dsn) = &state.runtime.dsn {
                let page = state.query.pagination.current_page;
                Some(vec![Effect::ExecutePreview {
                    dsn: dsn.clone(),
                    schema: state.query.pagination.schema.clone(),
                    table: state.query.pagination.table.clone(),
                    generation: state.cache.selection_generation,
                    limit: PREVIEW_PAGE_SIZE,
                    offset: page * PREVIEW_PAGE_SIZE,
                    target_page: page,
                }])
            } else {
                Some(vec![])
            }
        }

        Action::ExecuteWriteFailed(error) => {
            state.query.status = QueryStatus::Idle;
            state.query.start_time = None;
            state.pending_write_preview = None;
            state.messages.set_error_at(error.clone(), now);
            state.ui.input_mode = InputMode::CellEdit;
            Some(vec![])
        }

        Action::ResultNextPage => {
            let is_preview = state
                .query
                .current_result
                .as_ref()
                .is_some_and(|r| r.source == QuerySource::Preview);
            if state.query.status != QueryStatus::Idle || !is_preview {
                return Some(vec![]);
            }
            if !state.query.pagination.can_next() {
                return Some(vec![]);
            }
            if let Some(dsn) = &state.runtime.dsn {
                let next_page = state.query.pagination.current_page + 1;
                state.query.status = QueryStatus::Running;
                state.query.start_time = Some(now);
                state.ui.result_scroll_offset = 0;
                state.ui.result_horizontal_offset = 0;
                Some(vec![Effect::ExecutePreview {
                    dsn: dsn.clone(),
                    schema: state.query.pagination.schema.clone(),
                    table: state.query.pagination.table.clone(),
                    generation: state.cache.selection_generation,
                    limit: PREVIEW_PAGE_SIZE,
                    offset: next_page * PREVIEW_PAGE_SIZE,
                    target_page: next_page,
                }])
            } else {
                Some(vec![])
            }
        }

        Action::ResultPrevPage => {
            let is_preview = state
                .query
                .current_result
                .as_ref()
                .is_some_and(|r| r.source == QuerySource::Preview);
            if state.query.status != QueryStatus::Idle || !is_preview {
                return Some(vec![]);
            }
            if !state.query.pagination.can_prev() {
                return Some(vec![]);
            }
            if let Some(dsn) = &state.runtime.dsn {
                let prev_page = state.query.pagination.current_page - 1;
                state.query.status = QueryStatus::Running;
                state.query.start_time = Some(now);
                state.ui.result_scroll_offset = 0;
                state.ui.result_horizontal_offset = 0;
                // When going back, the page is not at the end anymore
                state.query.pagination.reached_end = false;
                Some(vec![Effect::ExecutePreview {
                    dsn: dsn.clone(),
                    schema: state.query.pagination.schema.clone(),
                    table: state.query.pagination.table.clone(),
                    generation: state.cache.selection_generation,
                    limit: PREVIEW_PAGE_SIZE,
                    offset: prev_page * PREVIEW_PAGE_SIZE,
                    target_page: prev_page,
                }])
            } else {
                Some(vec![])
            }
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::query_execution::PaginationState;
    use crate::domain::{Column, Index, IndexType, Table, Trigger, TriggerEvent, TriggerTiming};

    fn create_test_state() -> AppState {
        let mut state = AppState::new("test_project".to_string());
        state.runtime.dsn = Some("postgres://localhost/test".to_string());
        state
    }

    fn preview_result(row_count: usize) -> Arc<QueryResult> {
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
        })
    }

    fn adhoc_result() -> Arc<QueryResult> {
        Arc::new(QueryResult {
            query: "SELECT 1".to_string(),
            columns: vec!["id".to_string()],
            rows: vec![vec!["1".to_string()]],
            row_count: 1,
            execution_time_ms: 10,
            executed_at: Instant::now(),
            source: QuerySource::Adhoc,
            error: None,
        })
    }

    fn editable_preview_result() -> Arc<QueryResult> {
        Arc::new(QueryResult {
            query: "SELECT * FROM users".to_string(),
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![vec!["1".to_string(), "Alice".to_string()]],
            row_count: 1,
            execution_time_ms: 10,
            executed_at: Instant::now(),
            source: QuerySource::Preview,
            error: None,
        })
    }

    fn users_table_detail() -> Table {
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

    mod next_page {
        use super::*;

        #[test]
        fn emits_effect_with_correct_offset() {
            let mut state = create_test_state();
            state.query.current_result = Some(preview_result(PREVIEW_PAGE_SIZE));
            state.query.pagination = PaginationState {
                current_page: 0,
                total_rows_estimate: Some(1500),
                reached_end: false,
                schema: "public".to_string(),
                table: "users".to_string(),
            };
            let now = Instant::now();

            let effects = reduce_query(&mut state, &Action::ResultNextPage, now).unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::ExecutePreview {
                    offset,
                    target_page,
                    ..
                } => {
                    assert_eq!(*offset, PREVIEW_PAGE_SIZE);
                    assert_eq!(*target_page, 1);
                }
                other => panic!("expected ExecutePreview, got {:?}", other),
            }
        }

        #[test]
        fn noop_when_reached_end() {
            let mut state = create_test_state();
            state.query.current_result = Some(preview_result(100));
            state.query.pagination.reached_end = true;
            let now = Instant::now();

            let effects = reduce_query(&mut state, &Action::ResultNextPage, now).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn noop_for_adhoc() {
            let mut state = create_test_state();
            state.query.current_result = Some(adhoc_result());
            let now = Instant::now();

            let effects = reduce_query(&mut state, &Action::ResultNextPage, now).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn noop_when_running() {
            let mut state = create_test_state();
            state.query.current_result = Some(preview_result(PREVIEW_PAGE_SIZE));
            state.query.status = QueryStatus::Running;
            let now = Instant::now();

            let effects = reduce_query(&mut state, &Action::ResultNextPage, now).unwrap();

            assert!(effects.is_empty());
        }
    }

    mod prev_page {
        use super::*;

        #[test]
        fn emits_effect_with_correct_offset() {
            let mut state = create_test_state();
            state.query.current_result = Some(preview_result(PREVIEW_PAGE_SIZE));
            state.query.pagination = PaginationState {
                current_page: 2,
                total_rows_estimate: Some(1500),
                reached_end: false,
                schema: "public".to_string(),
                table: "users".to_string(),
            };
            let now = Instant::now();

            let effects = reduce_query(&mut state, &Action::ResultPrevPage, now).unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::ExecutePreview {
                    offset,
                    target_page,
                    ..
                } => {
                    assert_eq!(*offset, PREVIEW_PAGE_SIZE);
                    assert_eq!(*target_page, 1);
                }
                other => panic!("expected ExecutePreview, got {:?}", other),
            }
        }

        #[test]
        fn noop_on_first_page() {
            let mut state = create_test_state();
            state.query.current_result = Some(preview_result(PREVIEW_PAGE_SIZE));
            state.query.pagination.current_page = 0;
            let now = Instant::now();

            let effects = reduce_query(&mut state, &Action::ResultPrevPage, now).unwrap();

            assert!(effects.is_empty());
        }
    }

    mod execute_preview {
        use super::*;

        #[test]
        fn resets_pagination() {
            let mut state = create_test_state();
            state.query.pagination = PaginationState {
                current_page: 5,
                total_rows_estimate: Some(10000),
                reached_end: true,
                schema: "old_schema".to_string(),
                table: "old_table".to_string(),
            };
            let now = Instant::now();

            let _ = reduce_query(
                &mut state,
                &Action::ExecutePreview {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    generation: 1,
                },
                now,
            );

            assert_eq!(state.query.pagination.current_page, 0);
            assert!(!state.query.pagination.reached_end);
            assert_eq!(state.query.pagination.schema, "public");
            assert_eq!(state.query.pagination.table, "users");
        }
    }

    mod query_completed {
        use super::*;

        #[test]
        fn sets_page_and_reached_end() {
            let mut state = create_test_state();
            state.cache.selection_generation = 1;
            let result = preview_result(100); // Less than PAGE_SIZE
            let now = Instant::now();

            let _ = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result,
                    generation: 1,
                    target_page: Some(2),
                },
                now,
            );

            assert_eq!(state.query.pagination.current_page, 2);
            assert!(state.query.pagination.reached_end);
        }

        #[test]
        fn does_not_set_reached_end_for_full_page() {
            let mut state = create_test_state();
            state.cache.selection_generation = 1;
            let result = preview_result(PREVIEW_PAGE_SIZE);
            let now = Instant::now();

            let _ = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result,
                    generation: 1,
                    target_page: Some(0),
                },
                now,
            );

            assert_eq!(state.query.pagination.current_page, 0);
            assert!(!state.query.pagination.reached_end);
        }

        #[test]
        fn adhoc_does_not_update_pagination() {
            let mut state = create_test_state();
            state.query.pagination.current_page = 3;
            let result = adhoc_result();
            let now = Instant::now();

            let _ = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result,
                    generation: 0,
                    target_page: None,
                },
                now,
            );

            // Pagination unchanged for adhoc
            assert_eq!(state.query.pagination.current_page, 3);
        }
    }

    mod query_failed {
        use super::*;
        use crate::app::ui_state::ResultNavMode;

        #[test]
        fn resets_result_selection_and_offsets() {
            let mut state = create_test_state();
            state.cache.selection_generation = 1;
            state.ui.result_selection.enter_row(5);
            state.ui.result_selection.enter_cell(2);
            state.ui.result_scroll_offset = 10;
            state.ui.result_horizontal_offset = 3;

            let _ = reduce_query(
                &mut state,
                &Action::QueryFailed("error".to_string(), 1),
                Instant::now(),
            );

            assert_eq!(state.ui.result_selection.mode(), ResultNavMode::Scroll);
            assert_eq!(state.ui.result_scroll_offset, 0);
            assert_eq!(state.ui.result_horizontal_offset, 0);
        }
    }

    mod write_flow {
        use super::*;

        fn editable_state() -> AppState {
            let mut state = create_test_state();
            state.query.current_result = Some(editable_preview_result());
            state.cache.table_detail = Some(users_table_detail());
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.ui.input_mode = InputMode::CellEdit;
            state.cell_edit.begin(0, 1, "Alice".to_string());
            state.cell_edit.draft_value = "Bob".to_string();
            state
        }

        #[test]
        fn write_requires_cell_edit_mode() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::Normal;

            let effects = reduce_query(&mut state, &Action::SubmitCellEditWrite, Instant::now());
            assert!(effects.unwrap().is_empty());
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("`:w` is only available in Cell Edit mode")
            );
        }

        #[test]
        fn submit_write_opens_confirm_dialog() {
            let mut state = editable_state();

            let effects =
                reduce_query(&mut state, &Action::SubmitCellEditWrite, Instant::now()).unwrap();
            assert_eq!(effects.len(), 1);

            let dispatched = match &effects[0] {
                Effect::DispatchActions(actions) => actions.first().expect("action"),
                other => panic!("expected DispatchActions, got {:?}", other),
            };
            match dispatched {
                Action::OpenWritePreviewConfirm(preview) => {
                    assert!(preview.sql.contains("UPDATE"));
                }
                other => panic!("expected OpenWritePreviewConfirm, got {:?}", other),
            }
        }

        #[test]
        fn execute_write_success_refreshes_preview_page() {
            let mut state = editable_state();
            state.query.pagination.current_page = 2;

            let effects = reduce_query(
                &mut state,
                &Action::ExecuteWriteSucceeded { affected_rows: 1 },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::ExecutePreview {
                    offset,
                    target_page,
                    ..
                } => {
                    assert_eq!(*offset, 2 * PREVIEW_PAGE_SIZE);
                    assert_eq!(*target_page, 2);
                }
                other => panic!("expected ExecutePreview, got {:?}", other),
            }
        }

        #[test]
        fn execute_write_with_non_one_row_sets_error() {
            let mut state = editable_state();

            let effects = reduce_query(
                &mut state,
                &Action::ExecuteWriteSucceeded { affected_rows: 0 },
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.ui.input_mode, InputMode::CellEdit);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("UPDATE expected 1 row, but affected 0 rows")
            );
        }
    }

    mod sql_preview_formatting {
        use super::format_update_sql_for_modal;
        use crate::app::write_guardrails::{
            GuardrailDecision, RiskLevel, TargetSummary, WriteOperation, WritePreview,
        };
        use crate::domain::Column;
        use crate::domain::Table;

        fn table_detail() -> Table {
            Table {
                schema: "sales".to_string(),
                name: "shipping_zones".to_string(),
                owner: None,
                columns: vec![
                    Column {
                        name: "id".to_string(),
                        data_type: "integer".to_string(),
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
                        nullable: false,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: 2,
                    },
                ],
                primary_key: Some(vec!["id".to_string()]),
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            }
        }

        fn preview_sql() -> WritePreview {
            WritePreview {
                operation: WriteOperation::Update,
                sql:
                    "UPDATE \"sales\".\"shipping_zones\" SET \"name\" = 'North' WHERE \"id\" = '2';"
                        .to_string(),
                target_summary: TargetSummary {
                    schema: "sales".to_string(),
                    table: "shipping_zones".to_string(),
                    key_values: vec![("id".to_string(), "2".to_string())],
                },
                diff: vec![crate::app::write_guardrails::ColumnDiff {
                    column: "name".to_string(),
                    before: "North America".to_string(),
                    after: "North".to_string(),
                }],
                guardrail: GuardrailDecision {
                    risk_level: RiskLevel::Low,
                    blocked: false,
                    reason: None,
                    target_summary: None,
                },
            }
        }

        #[test]
        fn pretty_formats_update_and_unquotes_numeric_pk_for_display() {
            let preview = preview_sql();
            let formatted = format_update_sql_for_modal(&preview, Some(&table_detail()));
            assert_eq!(
                formatted,
                "UPDATE sales.shipping_zones\nSET name = 'North'\nWHERE id = 2;"
            );
        }

        #[test]
        fn keeps_quotes_for_non_simple_identifiers() {
            let mut preview = preview_sql();
            preview.target_summary.schema = "Sales".to_string();
            preview.target_summary.table = "order".to_string();
            preview.target_summary.key_values = vec![("User-ID".to_string(), "2".to_string())];
            preview.diff[0].column = "Display Name".to_string();

            let formatted = format_update_sql_for_modal(&preview, None);
            assert_eq!(
                formatted,
                "UPDATE \"Sales\".\"order\"\nSET \"Display Name\" = 'North'\nWHERE \"User-ID\" = '2';"
            );
        }
    }
}
