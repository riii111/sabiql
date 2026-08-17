use std::sync::Arc;

use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::{
    CommandTag, QueryValue, RefreshScope,
    mysql_sql::{
        MySqlStatement, MySqlStatementKind, classify_mysql_multi_statement,
        has_mysql_read_only_side_effect, mysql_export_plan, mysql_statement_is_data_modifying,
        mysql_statement_is_persistent_schema_change,
    },
};

use super::super::sql;
use super::xml::MySqlResultSet;

pub(super) const MYSQL_SESSION_MARKER_COLUMN: &str = "__sabiql_session_marker";

#[derive(Debug, Clone, Copy)]
pub(super) enum MySqlMetadataFallbackKind {
    Select,
    Table,
    Show,
    Describe,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::adapters::mysql) struct MySqlExecutionResult {
    pub(in crate::adapters::mysql) result_set: Option<MySqlResultSet>,
    pub(in crate::adapters::mysql) command_tag: Option<CommandTag>,
    pub(in crate::adapters::mysql) refresh_scope: RefreshScope,
}

pub(super) struct MySqlCommandEvent {
    pub(super) kind: MySqlStatementKind,
    pub(super) target: Option<String>,
    pub(super) tag: CommandTag,
}

pub(in crate::adapters::mysql) fn validate_mysql_multi_query(
    query: &str,
    selected_database: Option<&str>,
    access_mode: AccessMode,
) -> Result<Vec<MySqlStatement>, DbOperationError> {
    let statements = classify_mysql_multi_statement(query, selected_database)
        .map_err(DbOperationError::UnsupportedOperation)?;
    if access_mode.is_read_only() && !statements.iter().all(mysql_statement_is_read_only_allowed) {
        return Err(DbOperationError::PermissionDenied(
            "read-only mode blocks MySQL write statements".to_string(),
        ));
    }
    Ok(statements)
}

fn mysql_statement_is_read_only_allowed(statement: &MySqlStatement) -> bool {
    matches!(
        statement.kind,
        MySqlStatementKind::Select
            | MySqlStatementKind::Table
            | MySqlStatementKind::Show
            | MySqlStatementKind::Describe
    ) && !has_mysql_read_only_side_effect(&statement.sql).unwrap_or(true)
}

pub(in crate::adapters::mysql) fn validate_mysql_export_query(
    query: &str,
    selected_database: Option<&str>,
) -> Result<(), DbOperationError> {
    validate_mysql_multi_query(query, selected_database, AccessMode::ReadOnly)?;
    if mysql_export_plan(query).is_none() {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL CSV export supports a single read-only result query".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn mysql_metadata_fallback_kind(
    kind: &MySqlStatementKind,
) -> Option<MySqlMetadataFallbackKind> {
    match kind {
        MySqlStatementKind::Select => Some(MySqlMetadataFallbackKind::Select),
        MySqlStatementKind::Table => Some(MySqlMetadataFallbackKind::Table),
        MySqlStatementKind::Show => Some(MySqlMetadataFallbackKind::Show),
        MySqlStatementKind::Describe => Some(MySqlMetadataFallbackKind::Describe),
        _ => None,
    }
}

pub(super) fn mysql_metadata_fallback_has_unsupported_session_state(
    statements: &[MySqlStatement],
) -> bool {
    let mut temporary_table_created = false;
    for statement in statements {
        if temporary_table_created
            && matches!(
                statement.kind,
                MySqlStatementKind::Show | MySqlStatementKind::Describe
            )
        {
            return true;
        }
        if matches!(
            statement.kind,
            MySqlStatementKind::CreateTable { temporary: true }
        ) {
            temporary_table_created = true;
        }
    }
    false
}

pub(super) fn mysql_metadata_select_query(
    query: &str,
    source_alias: &str,
    marker_alias: &str,
) -> Result<String, DbOperationError> {
    let query = query.trim().trim_end_matches(';').trim_end();
    if query.is_empty() {
        return Err(DbOperationError::QueryFailed(
            "MySQL empty SELECT cannot be used for metadata fallback".to_string(),
        ));
    }
    if has_mysql_read_only_side_effect(query)
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?
    {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL SELECT metadata fallback cannot prove that the query is side-effect free"
                .to_string(),
        ));
    }
    Ok(sql::build_metadata_select_query(
        query,
        source_alias,
        marker_alias,
    ))
}

pub(super) fn validate_mysql_session_marker(
    result: &MySqlResultSet,
    marker: &str,
) -> Result<(), DbOperationError> {
    if result.columns != [MYSQL_SESSION_MARKER_COLUMN]
        || result.values.len() != 1
        || result.values[0].len() != 1
        || result.values[0][0].as_str() != Some(marker)
    {
        return Err(DbOperationError::QueryFailed(
            "mysql read-only session marker did not match".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn query_failed_after_change(
    error: DbOperationError,
    refresh_scope: RefreshScope,
) -> DbOperationError {
    if refresh_scope == RefreshScope::None {
        error
    } else {
        DbOperationError::QueryFailedAfterChange {
            source: Arc::new(error),
            refresh_scope,
        }
    }
}

pub(super) fn query_failed_after_mysql_statement(
    error: DbOperationError,
    refresh_scope: RefreshScope,
    possible_refresh_scope: RefreshScope,
) -> DbOperationError {
    let refresh_scope = if is_mysql_statement_failure(&error) {
        refresh_scope
    } else {
        possible_refresh_scope
    };
    query_failed_after_change(error, refresh_scope)
}

fn is_mysql_statement_failure(error: &DbOperationError) -> bool {
    matches!(
        error,
        DbOperationError::PermissionDenied(_)
            | DbOperationError::ForeignKeyViolation(_)
            | DbOperationError::UniqueViolation(_)
            | DbOperationError::LockTimeout(_)
            | DbOperationError::ObjectMissing(_)
            | DbOperationError::QueryFailed(_)
            | DbOperationError::Canceled(_)
    )
}

pub(super) fn is_mysql_row_count_marker(result: &MySqlResultSet, marker: &str) -> bool {
    result.columns == ["__sabiql_marker", "affected_rows"]
        && result.values.len() == 1
        && result.values[0].first().and_then(QueryValue::as_str) == Some(marker)
}

pub(super) fn mysql_row_count_marker(
    result: &MySqlResultSet,
    marker: &str,
) -> Result<i64, DbOperationError> {
    if !is_mysql_row_count_marker(result, marker) || result.values[0].len() != 2 {
        return Err(DbOperationError::QueryFailed(
            "mysql ROW_COUNT marker did not match the executed statement".to_string(),
        ));
    }
    let value = result.values[0][1].as_str().ok_or_else(|| {
        DbOperationError::QueryFailed("mysql ROW_COUNT marker was NULL".to_string())
    })?;
    value.parse::<i64>().map_err(|_| {
        DbOperationError::QueryFailed("mysql ROW_COUNT marker was not an integer".to_string())
    })
}

pub(super) fn mysql_command_tag(
    kind: &MySqlStatementKind,
    affected_rows: i64,
    user_result: Option<&MySqlResultSet>,
) -> CommandTag {
    let rows = || u64::try_from(affected_rows.max(0)).unwrap_or(0);
    match kind {
        MySqlStatementKind::Select
        | MySqlStatementKind::Table
        | MySqlStatementKind::Show
        | MySqlStatementKind::Describe => {
            CommandTag::Select(user_result.map_or(0, |result| result.values.len() as u64))
        }
        MySqlStatementKind::Insert | MySqlStatementKind::Replace => CommandTag::Insert(rows()),
        MySqlStatementKind::Update { .. } => CommandTag::Update(rows()),
        MySqlStatementKind::Delete { .. } => CommandTag::Delete(rows()),
        MySqlStatementKind::CreateTable { temporary: true } => {
            CommandTag::Other("CREATE TEMPORARY TABLE".to_string())
        }
        MySqlStatementKind::CreateTable { temporary: false } => {
            CommandTag::Create("TABLE".to_string())
        }
        MySqlStatementKind::AlterTable | MySqlStatementKind::RenameTable => {
            CommandTag::Alter("TABLE".to_string())
        }
        MySqlStatementKind::DropTable { temporary: true } => {
            CommandTag::Other("DROP TEMPORARY TABLE".to_string())
        }
        MySqlStatementKind::DropTable { temporary: false } => CommandTag::Drop("TABLE".to_string()),
        MySqlStatementKind::TruncateTable => CommandTag::Truncate,
        MySqlStatementKind::CreateView => CommandTag::Create("VIEW".to_string()),
        MySqlStatementKind::AlterView => CommandTag::Alter("VIEW".to_string()),
        MySqlStatementKind::DropView => CommandTag::Drop("VIEW".to_string()),
        MySqlStatementKind::CreateIndex => CommandTag::Create("INDEX".to_string()),
        MySqlStatementKind::DropIndex => CommandTag::Drop("INDEX".to_string()),
        MySqlStatementKind::Begin | MySqlStatementKind::StartTransaction => CommandTag::Begin,
        MySqlStatementKind::Commit => CommandTag::Commit,
        MySqlStatementKind::Rollback | MySqlStatementKind::RollbackToSavepoint => {
            CommandTag::Rollback
        }
        MySqlStatementKind::Savepoint => CommandTag::Other("SAVEPOINT".to_string()),
        MySqlStatementKind::ReleaseSavepoint => CommandTag::Other("RELEASE SAVEPOINT".to_string()),
    }
}

pub(super) fn mysql_refresh_scope(kind: &MySqlStatementKind) -> RefreshScope {
    if mysql_statement_is_persistent_schema_change(kind) {
        RefreshScope::Metadata
    } else if mysql_statement_is_data_modifying(kind) {
        RefreshScope::Data
    } else {
        RefreshScope::None
    }
}

#[derive(Default)]
struct MySqlPendingTransactionTags {
    data: Vec<CommandTag>,
    savepoints: Vec<(String, usize)>,
}

fn apply_pending_mysql_data(
    pending: MySqlPendingTransactionTags,
    committed_data: &mut Option<CommandTag>,
) {
    if let Some(tag) = pending.data.last() {
        *committed_data = Some(tag.clone());
    }
}

pub(super) fn aggregate_mysql_command_tag(events: &[MySqlCommandEvent]) -> Option<CommandTag> {
    let mut committed_schema = None;
    let mut committed_data = None;
    let mut pending = None;
    let mut last_tag = None;

    for event in events {
        last_tag = Some(event.tag.clone());
        match &event.kind {
            MySqlStatementKind::Begin | MySqlStatementKind::StartTransaction => {
                pending = Some(MySqlPendingTransactionTags::default());
            }
            MySqlStatementKind::Commit => {
                if let Some(transaction) = pending.take() {
                    apply_pending_mysql_data(transaction, &mut committed_data);
                }
            }
            MySqlStatementKind::Rollback => {
                pending = None;
            }
            MySqlStatementKind::Savepoint => {
                if let Some(transaction) = pending.as_mut()
                    && let Some(name) = event.target.as_deref()
                {
                    transaction
                        .savepoints
                        .retain(|(current, _)| !current.eq_ignore_ascii_case(name));
                    transaction
                        .savepoints
                        .push((name.to_string(), transaction.data.len()));
                }
            }
            MySqlStatementKind::RollbackToSavepoint => {
                if let Some(transaction) = pending.as_mut()
                    && let Some(name) = event.target.as_deref()
                    && let Some(index) = transaction
                        .savepoints
                        .iter()
                        .position(|(current, _)| current.eq_ignore_ascii_case(name))
                {
                    transaction.data.truncate(transaction.savepoints[index].1);
                    transaction.savepoints.truncate(index + 1);
                }
            }
            MySqlStatementKind::ReleaseSavepoint => {
                if let Some(transaction) = pending.as_mut()
                    && let Some(name) = event.target.as_deref()
                    && let Some(index) = transaction
                        .savepoints
                        .iter()
                        .position(|(current, _)| current.eq_ignore_ascii_case(name))
                {
                    transaction.savepoints.remove(index);
                }
            }
            MySqlStatementKind::CreateTable { temporary: true }
            | MySqlStatementKind::DropTable { temporary: true } => {}
            kind if mysql_statement_is_persistent_schema_change(kind) => {
                if let Some(transaction) = pending.take() {
                    apply_pending_mysql_data(transaction, &mut committed_data);
                }
                committed_schema = Some(event.tag.clone());
            }
            kind if mysql_statement_is_data_modifying(kind) => {
                if let Some(transaction) = pending.as_mut() {
                    transaction.data.push(event.tag.clone());
                } else {
                    committed_data = Some(event.tag.clone());
                }
            }
            _ => {}
        }
    }

    committed_schema.or(committed_data).or(last_tag)
}

#[cfg(test)]
mod tests {
    use crate::app::ports::outbound::UnsupportedOperationKind;

    use super::super::error::validate_mode_probe;
    use super::*;

    #[test]
    fn csv_export_accepts_one_read_only_result_query() {
        assert!(validate_mysql_export_query("SELECT 1", Some("app")).is_ok());
        for query in ["TABLE users", "SHOW TABLES", "DESCRIBE users"] {
            assert!(
                validate_mysql_export_query(query, Some("app")).is_ok(),
                "{query}"
            );
        }
        assert!(matches!(
            validate_mysql_export_query("INSERT INTO users VALUES (1)", Some("app")),
            Err(DbOperationError::PermissionDenied(_))
        ));
        assert!(matches!(
            validate_mysql_export_query("SELECT 1; SELECT 2", Some("app")),
            Err(DbOperationError::UnsupportedOperation(details))
                if details.contains("single read-only result")
        ));
        assert!(matches!(
            validate_mysql_export_query("SELECT GET_LOCK('sabiql', 0)", Some("app")),
            Err(DbOperationError::PermissionDenied(_))
        ));
    }

    #[test]
    fn mode_probe_requires_marker_and_allowed_mode_before_user_sql() {
        let probe = MySqlResultSet {
            columns: vec![
                "__sabiql_probe".to_string(),
                "__sabiql_sql_mode".to_string(),
            ],
            values: vec![vec![
                QueryValue::Text("marker".to_string()),
                QueryValue::Text("STRICT_TRANS_TABLES".to_string()),
            ]],
        };
        assert!(validate_mode_probe(&probe, "marker").is_ok());

        let mut unsupported = probe;
        unsupported.values[0][1] = QueryValue::Text("ANSI_QUOTES".to_string());
        assert!(matches!(
            validate_mode_probe(&unsupported, "marker"),
            Err(DbOperationError::UnsupportedOperationWithKind {
                kind: UnsupportedOperationKind::SessionMode,
                ..
            })
        ));
    }
    #[test]
    fn metadata_only_select_rejects_known_side_effects() {
        for query in [
            "SELECT value FROM items FOR UPDATE",
            "SELECT GET_LOCK('sabiql', 0)",
            "SELECT @value := 1",
            "SELECT value FROM items WHERE FALSE FOR UPDATE",
            "SELECT GET_LOCK('sabiql', 0) WHERE FALSE",
            "SELECT `GET_LOCK`('sabiql', 0) WHERE FALSE",
            "SELECT `RELEASE_LOCK`('sabiql') WHERE FALSE",
            "SELECT `RELEASE_ALL_LOCKS`() WHERE FALSE",
            "SELECT @value := 1 WHERE FALSE",
        ] {
            assert!(
                mysql_metadata_select_query(query, "__source", "__marker").is_err(),
                "{query}"
            );
        }
        for query in [
            "WITH cte_rows AS (SELECT 1 AS first_alias) SELECT first_alias FROM cte_rows WHERE FALSE",
            "WITH cte_rows(first_alias) AS (SELECT 1) SELECT first_alias FROM cte_rows WHERE FALSE",
            "SELECT CASE (1) WHEN 1 THEN 'x' ELSE 'y' END AS value WHERE FALSE",
            "SELECT CONCAT('a', 'b') AS value WHERE FALSE",
            "SELECT CONCAT/**/('a', 'b') AS value WHERE FALSE",
            "SELECT CAST(1 AS CHAR) AS value WHERE FALSE",
            "SELECT CONVERT(1, CHAR) AS value WHERE FALSE",
            "SELECT EXTRACT(YEAR FROM CURRENT_DATE) AS value WHERE FALSE",
        ] {
            assert!(
                mysql_metadata_select_query(query, "__source", "__marker").is_ok(),
                "{query}"
            );
        }
    }

    #[test]
    fn metadata_fallback_wraps_source_query_with_limit_zero() {
        let source_query = "SELECT SLEEP(10) AS sleep_value WHERE FALSE";

        let fallback_query =
            mysql_metadata_select_query(source_query, "__source", "__marker").unwrap();

        assert!(
            fallback_query.contains(
                "SELECT * FROM ((SELECT SLEEP(10) AS sleep_value WHERE FALSE\n) LIMIT 0)"
            )
        );
        assert_eq!(fallback_query.matches("SELECT SLEEP(10)").count(), 1);
    }

    #[test]
    fn failure_before_a_change_keeps_original_error() {
        let error = query_failed_after_change(
            DbOperationError::ForeignKeyViolation("foreign key failed".to_string()),
            RefreshScope::None,
        );

        assert!(matches!(
            error,
            DbOperationError::ForeignKeyViolation(details) if details == "foreign key failed"
        ));
    }

    #[test]
    fn read_only_rejects_temporary_table_dml_before_starting_mysql() {
        let directory = tempfile::tempdir().unwrap();
        let log_file = directory.path().join("mysql.log");
        let query = "CREATE TEMPORARY TABLE temp_items (id INT); INSERT INTO temp_items VALUES (1); DROP TEMPORARY TABLE temp_items";

        let result = validate_mysql_multi_query(query, Some("app"), AccessMode::ReadOnly);

        assert!(matches!(
            result,
            Err(DbOperationError::PermissionDenied(details))
                if details.contains("read-only mode blocks MySQL write statements")
        ));
        assert!(!log_file.exists());
    }

    #[test]
    fn read_only_rejects_read_write_overrides_before_starting_mysql() {
        for query in [
            "SET SESSION TRANSACTION READ WRITE",
            "START TRANSACTION READ WRITE",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let log_file = directory.path().join("mysql.log");

            let result = validate_mysql_multi_query(query, Some("app"), AccessMode::ReadOnly);

            assert!(matches!(
                result,
                Err(DbOperationError::UnsupportedOperation(_))
            ));
            assert!(!log_file.exists(), "{query}");
        }
    }

    #[test]
    fn read_only_rejects_quoted_side_effect_functions_before_starting_mysql() {
        for query in [
            "SELECT `GET_LOCK`('sabiql', 0)",
            "SELECT `RELEASE_LOCK`('sabiql')",
            "SELECT `RELEASE_ALL_LOCKS`()",
        ] {
            let result = validate_mysql_multi_query(query, Some("app"), AccessMode::ReadOnly);

            assert!(
                matches!(
                    result,
                    Err(DbOperationError::PermissionDenied(details))
                        if details.contains("read-only mode blocks MySQL write statements")
                ),
                "{query}"
            );
        }
    }

    #[test]
    fn rejects_empty_metadata_fallback_after_temporary_table_creation() {
        for query in [
            "CREATE TEMPORARY TABLE temp_items (id INT); DESCRIBE temp_items 'missing'; DROP TEMPORARY TABLE temp_items",
            "CREATE TEMPORARY TABLE temp_items (id INT); SHOW COLUMNS FROM temp_items LIKE 'missing'; DROP TEMPORARY TABLE temp_items",
        ] {
            let statements = validate_mysql_multi_query(query, Some("app"), AccessMode::ReadWrite)
                .expect("query should be classified before the session-state check");

            assert!(mysql_metadata_fallback_has_unsupported_session_state(
                &statements
            ));
        }

        let statements = validate_mysql_multi_query(
            "SHOW COLUMNS FROM items",
            Some("app"),
            AccessMode::ReadWrite,
        )
        .expect("single SHOW should be classified");
        assert!(!mysql_metadata_fallback_has_unsupported_session_state(
            &statements
        ));
    }

    #[test]
    fn refresh_scope_ignores_temporary_table_ddl() {
        for kind in [
            MySqlStatementKind::CreateTable { temporary: true },
            MySqlStatementKind::DropTable { temporary: true },
        ] {
            assert_eq!(mysql_refresh_scope(&kind), RefreshScope::None);
        }
    }

    #[test]
    fn refresh_scope_preserves_data_and_persistent_metadata_changes() {
        assert_eq!(
            mysql_refresh_scope(&MySqlStatementKind::Insert),
            RefreshScope::Data
        );
        assert_eq!(
            mysql_refresh_scope(&MySqlStatementKind::CreateTable { temporary: false }),
            RefreshScope::Metadata
        );
    }

    #[test]
    fn transaction_rollback_removes_pending_data_tag() {
        let events = vec![
            MySqlCommandEvent {
                kind: MySqlStatementKind::Begin,
                target: None,
                tag: CommandTag::Begin,
            },
            MySqlCommandEvent {
                kind: MySqlStatementKind::Update { has_where: true },
                target: Some("items".to_string()),
                tag: CommandTag::Update(1),
            },
            MySqlCommandEvent {
                kind: MySqlStatementKind::Rollback,
                target: None,
                tag: CommandTag::Rollback,
            },
            MySqlCommandEvent {
                kind: MySqlStatementKind::Select,
                target: None,
                tag: CommandTag::Select(1),
            },
        ];

        assert_eq!(
            aggregate_mysql_command_tag(&events),
            Some(CommandTag::Select(1))
        );
    }

    #[test]
    fn ddl_implicit_commit_keeps_prior_data_change() {
        let events = vec![
            MySqlCommandEvent {
                kind: MySqlStatementKind::Begin,
                target: None,
                tag: CommandTag::Begin,
            },
            MySqlCommandEvent {
                kind: MySqlStatementKind::Insert,
                target: Some("items".to_string()),
                tag: CommandTag::Insert(1),
            },
            MySqlCommandEvent {
                kind: MySqlStatementKind::CreateTable { temporary: false },
                target: Some("created".to_string()),
                tag: CommandTag::Create("TABLE".to_string()),
            },
            MySqlCommandEvent {
                kind: MySqlStatementKind::Rollback,
                target: None,
                tag: CommandTag::Rollback,
            },
        ];

        assert_eq!(
            aggregate_mysql_command_tag(&events),
            Some(CommandTag::Create("TABLE".to_string()))
        );
    }
}
