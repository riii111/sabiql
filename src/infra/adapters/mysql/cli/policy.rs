use std::sync::Arc;

use crate::app::policy::sql::mysql_statement::{
    MysqlStatement, MysqlStatementKind, classify_mysql_statement, has_mysql_read_only_side_effect,
};
use crate::app::policy::write::sql_risk::{
    MultiStatementDecision, evaluate_mysql_multi_statement, mysql_statement_is_data_modifying,
    mysql_statement_is_schema_modifying,
};
use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::{CommandTag, QueryValue, RefreshScope};

use super::super::sql;
use super::xml::MysqlResultSet;

pub(super) const MYSQL_SESSION_MARKER_COLUMN: &str = "__sabiql_session_marker";

#[derive(Debug, Clone, Copy)]
pub(super) enum MysqlMetadataFallbackKind {
    Select,
    Show,
    Describe,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::adapters::mysql) struct MysqlExecutionResult {
    pub(in crate::adapters::mysql) result_set: Option<MysqlResultSet>,
    pub(in crate::adapters::mysql) command_tag: Option<CommandTag>,
    pub(in crate::adapters::mysql) refresh_scope: RefreshScope,
}

pub(super) struct MysqlCommandEvent {
    pub(super) kind: MysqlStatementKind,
    pub(super) target: Option<String>,
    pub(super) tag: CommandTag,
}

pub(in crate::adapters::mysql) fn validate_mysql_multi_query(
    query: &str,
    selected_database: Option<&str>,
    access_mode: AccessMode,
) -> Result<Vec<MysqlStatement>, DbOperationError> {
    let decision = evaluate_mysql_multi_statement(query, selected_database);
    let (statements, risk) = match decision {
        MultiStatementDecision::Allow { statements, risk } => (statements, risk),
        MultiStatementDecision::Block { reason } => {
            return Err(DbOperationError::UnsupportedOperation(reason));
        }
    };
    if access_mode.is_read_only() && !risk.read_only_allowed {
        return Err(DbOperationError::PermissionDenied(
            "read-only mode blocks MySQL write statements".to_string(),
        ));
    }
    statements
        .iter()
        .map(|statement| {
            classify_mysql_statement(statement)
                .map_err(|error| DbOperationError::UnsupportedOperation(error.to_string()))
        })
        .collect()
}

pub(in crate::adapters::mysql) fn validate_mysql_export_query(
    query: &str,
    selected_database: Option<&str>,
) -> Result<(), DbOperationError> {
    let statements = validate_mysql_multi_query(query, selected_database, AccessMode::ReadOnly)?;
    if statements.len() != 1
        || !matches!(
            statements[0].kind,
            MysqlStatementKind::Select
                | MysqlStatementKind::Table
                | MysqlStatementKind::Show
                | MysqlStatementKind::Describe
        )
    {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL CSV export supports a single read-only result query".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn mysql_metadata_fallback_kind(
    kind: &MysqlStatementKind,
) -> Option<MysqlMetadataFallbackKind> {
    match kind {
        MysqlStatementKind::Select => Some(MysqlMetadataFallbackKind::Select),
        MysqlStatementKind::Show => Some(MysqlMetadataFallbackKind::Show),
        MysqlStatementKind::Describe => Some(MysqlMetadataFallbackKind::Describe),
        _ => None,
    }
}

pub(super) fn mysql_metadata_fallback_has_unsupported_session_state(
    statements: &[MysqlStatement],
) -> bool {
    let mut temporary_table_created = false;
    for statement in statements {
        if temporary_table_created
            && matches!(
                statement.kind,
                MysqlStatementKind::Show | MysqlStatementKind::Describe
            )
        {
            return true;
        }
        if matches!(
            statement.kind,
            MysqlStatementKind::CreateTable { temporary: true }
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
    if mysql_metadata_select_has_unproven_function(query) {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL SELECT metadata fallback cannot prove that function calls are side-effect free"
                .to_string(),
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

fn mysql_metadata_select_has_unproven_function(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'#'
            || (bytes.get(index..index + 2) == Some(b"--")
                && bytes.get(index + 2).is_none_or(u8::is_ascii_whitespace))
        {
            index = bytes[index..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |offset| index + offset + 1);
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            index = bytes
                .get(index + 2..)
                .and_then(|rest| rest.windows(2).position(|window| window == b"*/"))
                .map_or(bytes.len(), |offset| index + offset + 4);
            continue;
        }
        if bytes[index] == b'@' {
            return true;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            let quote = bytes[index];
            index = skip_mysql_metadata_quoted(bytes, index, quote);
            let next = skip_mysql_metadata_trivia(bytes, index);
            if quote == b'`' && bytes.get(next) == Some(&b'(') {
                return true;
            }
            continue;
        }
        if bytes[index].is_ascii_alphabetic() || matches!(bytes[index], b'_' | b'$') {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'$'))
            {
                index += 1;
            }
            let next = skip_mysql_metadata_trivia(bytes, index);
            if bytes.get(next) == Some(&b'(') {
                let name = &sql[start..index];
                let cte_column_list = mysql_metadata_is_cte_column_list(sql, next);
                let qualified = mysql_metadata_has_qualifier(bytes, start);
                if !cte_column_list
                    && (qualified
                        || !name.eq_ignore_ascii_case("SLEEP")
                            && !matches!(
                                name.to_ascii_uppercase().as_str(),
                                "AS" | "CASE" | "IN" | "EXISTS" | "OVER"
                            ))
                {
                    return true;
                }
            }
            continue;
        }
        index += 1;
    }
    false
}

fn skip_mysql_metadata_trivia(bytes: &[u8], mut index: usize) -> usize {
    loop {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) == Some(&b'#')
            || (bytes.get(index..index + 2) == Some(b"--")
                && bytes.get(index + 2).is_some_and(u8::is_ascii_whitespace))
        {
            index = bytes[index..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |offset| index + offset + 1);
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            index = bytes
                .get(index + 2..)
                .and_then(|rest| rest.windows(2).position(|window| window == b"*/"))
                .map_or(bytes.len(), |offset| index + offset + 4);
            continue;
        }
        return index;
    }
}

fn mysql_metadata_has_qualifier(bytes: &[u8], end: usize) -> bool {
    let mut index = 0;
    let mut previous = None;
    while index < end {
        if bytes[index] == b'#'
            || (bytes.get(index..index + 2) == Some(b"--")
                && bytes.get(index + 2).is_some_and(u8::is_ascii_whitespace))
            || bytes.get(index..index + 2) == Some(b"/*")
        {
            let next = skip_mysql_metadata_trivia(bytes, index);
            if next == index {
                index += 1;
            } else {
                index = next;
            }
            continue;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            index = skip_mysql_metadata_quoted(bytes, index, bytes[index]);
            previous = Some(b'\'');
        } else if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else {
            previous = Some(bytes[index]);
            index += 1;
        }
    }
    previous == Some(b'.')
}

fn mysql_metadata_is_cte_column_list(sql: &str, candidate_open: usize) -> bool {
    let bytes = sql.as_bytes();
    let mut index = skip_mysql_metadata_trivia(bytes, 0);
    let Some(after_with) = mysql_metadata_keyword_end(bytes, index, "WITH") else {
        return false;
    };
    index = skip_mysql_metadata_trivia(bytes, after_with);
    if let Some(after_recursive) = mysql_metadata_keyword_end(bytes, index, "RECURSIVE") {
        index = skip_mysql_metadata_trivia(bytes, after_recursive);
    }

    loop {
        index = match mysql_metadata_cte_name_end(bytes, index) {
            Some(end) => skip_mysql_metadata_trivia(bytes, end),
            None => return false,
        };
        let mut column_list = None;
        if bytes.get(index) == Some(&b'(') {
            let Some(end) = mysql_metadata_parenthesized_end(bytes, index) else {
                return false;
            };
            column_list = Some(index);
            index = skip_mysql_metadata_trivia(bytes, end);
        }
        let Some(after_as) = mysql_metadata_keyword_end(bytes, index, "AS") else {
            return false;
        };
        index = skip_mysql_metadata_trivia(bytes, after_as);
        let Some(body_end) = bytes
            .get(index)
            .filter(|byte| **byte == b'(')
            .and_then(|_| mysql_metadata_parenthesized_end(bytes, index))
        else {
            return false;
        };
        if column_list == Some(candidate_open) {
            return true;
        }
        index = skip_mysql_metadata_trivia(bytes, body_end);
        if bytes.get(index) != Some(&b',') {
            return false;
        }
        index = skip_mysql_metadata_trivia(bytes, index + 1);
    }
}

fn mysql_metadata_cte_name_end(bytes: &[u8], index: usize) -> Option<usize> {
    if bytes.get(index) == Some(&b'`') {
        Some(skip_mysql_metadata_quoted(bytes, index, b'`'))
    } else if bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$'))
    {
        Some(
            index
                + 1
                + bytes[index + 1..]
                    .iter()
                    .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
                    .count(),
        )
    } else {
        None
    }
}

fn mysql_metadata_keyword_end(bytes: &[u8], index: usize, keyword: &str) -> Option<usize> {
    let end = index.checked_add(keyword.len())?;
    if bytes
        .get(index..end)?
        .eq_ignore_ascii_case(keyword.as_bytes())
        && !bytes
            .get(end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
    {
        Some(end)
    } else {
        None
    }
}

fn mysql_metadata_parenthesized_end(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut index = open;
    while index < bytes.len() {
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            index = skip_mysql_metadata_quoted(bytes, index, bytes[index]);
            continue;
        }
        let next = skip_mysql_metadata_trivia(bytes, index);
        if next != index {
            index = next;
            continue;
        }
        match bytes[index] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn skip_mysql_metadata_quoted(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' && quote != b'`' {
            index = (index + 2).min(bytes.len());
        } else if bytes[index] == quote {
            if bytes.get(index + 1) == Some(&quote) {
                index += 2;
            } else {
                return index + 1;
            }
        } else {
            index += 1;
        }
    }
    bytes.len()
}

pub(super) fn validate_mysql_session_marker(
    result: &MysqlResultSet,
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

pub(super) fn is_mysql_row_count_marker(result: &MysqlResultSet, marker: &str) -> bool {
    result.columns == ["__sabiql_marker", "affected_rows"]
        && result.values.len() == 1
        && result.values[0].first().and_then(QueryValue::as_str) == Some(marker)
}

pub(super) fn mysql_row_count_marker(
    result: &MysqlResultSet,
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
    kind: &MysqlStatementKind,
    affected_rows: i64,
    user_result: Option<&MysqlResultSet>,
) -> CommandTag {
    let rows = || u64::try_from(affected_rows.max(0)).unwrap_or(0);
    match kind {
        MysqlStatementKind::Select
        | MysqlStatementKind::Table
        | MysqlStatementKind::Show
        | MysqlStatementKind::Describe => {
            CommandTag::Select(user_result.map_or(0, |result| result.values.len() as u64))
        }
        MysqlStatementKind::Insert | MysqlStatementKind::Replace => CommandTag::Insert(rows()),
        MysqlStatementKind::Update { .. } => CommandTag::Update(rows()),
        MysqlStatementKind::Delete { .. } => CommandTag::Delete(rows()),
        MysqlStatementKind::CreateTable { temporary: true } => {
            CommandTag::Other("CREATE TEMPORARY TABLE".to_string())
        }
        MysqlStatementKind::CreateTable { temporary: false } => {
            CommandTag::Create("TABLE".to_string())
        }
        MysqlStatementKind::AlterTable => CommandTag::Alter("TABLE".to_string()),
        MysqlStatementKind::DropTable { temporary: true } => {
            CommandTag::Other("DROP TEMPORARY TABLE".to_string())
        }
        MysqlStatementKind::DropTable { temporary: false } => CommandTag::Drop("TABLE".to_string()),
        MysqlStatementKind::TruncateTable => CommandTag::Truncate,
        MysqlStatementKind::CreateView => CommandTag::Create("VIEW".to_string()),
        MysqlStatementKind::DropView => CommandTag::Drop("VIEW".to_string()),
        MysqlStatementKind::CreateIndex => CommandTag::Create("INDEX".to_string()),
        MysqlStatementKind::DropIndex => CommandTag::Drop("INDEX".to_string()),
        MysqlStatementKind::Begin | MysqlStatementKind::StartTransaction => CommandTag::Begin,
        MysqlStatementKind::Commit => CommandTag::Commit,
        MysqlStatementKind::Rollback | MysqlStatementKind::RollbackToSavepoint => {
            CommandTag::Rollback
        }
        MysqlStatementKind::Savepoint => CommandTag::Other("SAVEPOINT".to_string()),
        MysqlStatementKind::ReleaseSavepoint => CommandTag::Other("RELEASE SAVEPOINT".to_string()),
    }
}

pub(super) fn mysql_refresh_scope(kind: &MysqlStatementKind) -> RefreshScope {
    if mysql_statement_is_schema_modifying(kind) {
        RefreshScope::Metadata
    } else if mysql_statement_is_data_modifying(kind) {
        RefreshScope::Data
    } else {
        RefreshScope::None
    }
}

fn mysql_statement_is_persistent_schema_change(kind: &MysqlStatementKind) -> bool {
    mysql_statement_is_schema_modifying(kind)
        && !matches!(
            kind,
            MysqlStatementKind::CreateTable { temporary: true }
                | MysqlStatementKind::DropTable { temporary: true }
        )
}

#[derive(Default)]
struct MysqlPendingTransactionTags {
    data: Vec<CommandTag>,
    savepoints: Vec<(String, usize)>,
}

fn apply_pending_mysql_data(
    pending: MysqlPendingTransactionTags,
    committed_data: &mut Option<CommandTag>,
) {
    if let Some(tag) = pending.data.last() {
        *committed_data = Some(tag.clone());
    }
}

pub(super) fn aggregate_mysql_command_tag(events: &[MysqlCommandEvent]) -> Option<CommandTag> {
    let mut committed_schema = None;
    let mut committed_data = None;
    let mut pending = None;
    let mut last_tag = None;

    for event in events {
        last_tag = Some(event.tag.clone());
        match &event.kind {
            MysqlStatementKind::Begin | MysqlStatementKind::StartTransaction => {
                pending = Some(MysqlPendingTransactionTags::default());
            }
            MysqlStatementKind::Commit => {
                if let Some(transaction) = pending.take() {
                    apply_pending_mysql_data(transaction, &mut committed_data);
                }
            }
            MysqlStatementKind::Rollback => {
                pending = None;
            }
            MysqlStatementKind::Savepoint => {
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
            MysqlStatementKind::RollbackToSavepoint => {
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
            MysqlStatementKind::ReleaseSavepoint => {
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
            MysqlStatementKind::CreateTable { temporary: true }
            | MysqlStatementKind::DropTable { temporary: true } => {}
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
    use crate::app::ports::outbound::MYSQL_SQL_MODE_UNSUPPORTED_MARKER;

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
    }

    #[test]
    fn mode_probe_requires_marker_and_allowed_mode_before_user_sql() {
        let probe = MysqlResultSet {
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
            Err(DbOperationError::UnsupportedOperation(details))
                if details.contains(MYSQL_SQL_MODE_UNSUPPORTED_MARKER)
        ));
    }
    #[test]
    fn metadata_only_select_rejects_known_side_effects() {
        for query in [
            "SELECT value FROM items FOR UPDATE",
            "SELECT GET_LOCK('sabiql', 0)",
            "SELECT @value := 1",
        ] {
            assert!(
                mysql_metadata_select_query(query, "__source", "__marker").is_err(),
                "{query}"
            );
        }
        assert!(mysql_metadata_select_query(
            "WITH cte_rows AS (SELECT 1 AS first_alias) SELECT first_alias FROM cte_rows WHERE FALSE",
            "__source",
            "__marker"
        )
        .is_ok());
        assert!(
            mysql_metadata_select_query(
                "SELECT CONCAT('a', 'b') AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT CONCAT/**/('a', 'b') AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT sabiql_test.user_function() AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT sabiql_test/**/.user_function() AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT `user_function`/**/() AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT @session_value AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT INTERVAL(10, 1, 5) AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "WITH cte_rows(first_alias) AS (SELECT 1) SELECT first_alias FROM cte_rows WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_ok()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT CASE (1) WHEN 1 THEN 'x' ELSE 'y' END AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_ok()
        );
        for query in [
            "SELECT CAST(1 AS CHAR) AS value WHERE FALSE",
            "SELECT CONVERT(1, CHAR) AS value WHERE FALSE",
            "SELECT EXTRACT(YEAR FROM CURRENT_DATE) AS value WHERE FALSE",
        ] {
            assert!(
                mysql_metadata_select_query(query, "__source", "__marker").is_err(),
                "{query}"
            );
        }
        assert!(
            mysql_metadata_select_query(
                "SELECT SLEEP(1) AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_ok()
        );
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
    fn transaction_rollback_removes_pending_data_tag() {
        let events = vec![
            MysqlCommandEvent {
                kind: MysqlStatementKind::Begin,
                target: None,
                tag: CommandTag::Begin,
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Update { has_where: true },
                target: Some("items".to_string()),
                tag: CommandTag::Update(1),
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Rollback,
                target: None,
                tag: CommandTag::Rollback,
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Select,
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
            MysqlCommandEvent {
                kind: MysqlStatementKind::Begin,
                target: None,
                tag: CommandTag::Begin,
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Insert,
                target: Some("items".to_string()),
                tag: CommandTag::Insert(1),
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::CreateTable { temporary: false },
                target: Some("created".to_string()),
                tag: CommandTag::Create("TABLE".to_string()),
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Rollback,
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
