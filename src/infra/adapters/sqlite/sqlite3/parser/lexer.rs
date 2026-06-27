use std::borrow::Cow;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::app::ports::outbound::DbOperationError;

fn is_ident_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn skip_quoted(bytes: &[u8], mut i: usize, quote: u8) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            if i + 1 < bytes.len() && bytes[i + 1] == quote {
                i += 2;
            } else {
                return i + 1;
            }
        } else {
            i += 1;
        }
    }
    i
}

fn skip_bracket_quoted(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b']' {
            return i + 1;
        }
        i += 1;
    }
    i
}

/// Returns the next SQL keyword and the byte offset immediately after it.
fn next_keyword_from(sql: &str, mut i: usize) -> Option<(&str, usize)> {
    let bytes = sql.as_bytes();
    while i < bytes.len() {
        match bytes[i] {
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    i += 2;
                }
            }
            b'\'' | b'"' | b'`' => {
                i = skip_quoted(bytes, i, bytes[i]);
            }
            b'[' => {
                i = skip_bracket_quoted(bytes, i);
            }
            b if b.is_ascii_alphabetic() => {
                let start = i;
                while i < bytes.len() && is_ident_char(bytes[i]) {
                    i += 1;
                }
                return Some((&sql[start..i], i));
            }
            _ => i += 1,
        }
    }
    None
}

pub(super) fn first_keyword(sql: &str) -> &str {
    next_keyword_from(sql, 0).map_or("", |(keyword, _)| keyword)
}

pub(super) fn second_keyword(sql: &str) -> Option<&str> {
    let (_, end) = next_keyword_from(sql, 0)?;
    next_keyword_from(sql, end).map(|(keyword, _)| keyword)
}

fn contains_keyword(sql: &str, expected: &str) -> bool {
    let mut offset = 0;
    while let Some((keyword, end)) = next_keyword_from(sql, offset) {
        if keyword.eq_ignore_ascii_case(expected) {
            return true;
        }
        offset = end;
    }
    false
}

fn is_create_trigger_prefix(sql: &str) -> bool {
    let Some((first, pos)) = next_keyword_from(sql, 0) else {
        return false;
    };
    if !first.eq_ignore_ascii_case("CREATE") {
        return false;
    }
    let Some((second, pos)) = next_keyword_from(sql, pos) else {
        return false;
    };
    if second.eq_ignore_ascii_case("TEMP") || second.eq_ignore_ascii_case("TEMPORARY") {
        let Some((third, _)) = next_keyword_from(sql, pos) else {
            return false;
        };
        return third.eq_ignore_ascii_case("TRIGGER");
    }
    second.eq_ignore_ascii_case("TRIGGER")
}

pub(in crate::adapters::sqlite::sqlite3) fn is_create_virtual_table_prefix(sql: &str) -> bool {
    let Some((first, pos)) = next_keyword_from(sql, 0) else {
        return false;
    };
    if !first.eq_ignore_ascii_case("CREATE") {
        return false;
    }
    let Some((second, pos)) = next_keyword_from(sql, pos) else {
        return false;
    };
    if !second.eq_ignore_ascii_case("VIRTUAL") {
        return false;
    }
    let Some((third, _)) = next_keyword_from(sql, pos) else {
        return false;
    };
    third.eq_ignore_ascii_case("TABLE")
}

pub(in crate::adapters::sqlite::sqlite3) fn virtual_table_module_name(sql: &str) -> Option<String> {
    let mut offset = 0;
    while let Some((keyword, end)) = next_keyword_from(sql, offset) {
        if keyword.eq_ignore_ascii_case("USING") {
            return module_name_at(sql, end);
        }
        offset = end;
    }
    None
}

fn module_name_at(sql: &str, start: usize) -> Option<String> {
    let bytes = sql.as_bytes();
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    match bytes[i] {
        b'\'' | b'"' | b'`' => {
            let quote = bytes[i];
            i += 1;
            let name_start = i;
            while i < bytes.len() {
                if bytes[i] == quote {
                    if i + 1 < bytes.len() && bytes[i + 1] == quote {
                        i += 2;
                    } else {
                        let name = sql[name_start..i].trim();
                        return if name.is_empty() {
                            None
                        } else {
                            Some(name.to_string())
                        };
                    }
                } else {
                    i += 1;
                }
            }
            None
        }
        b'[' => {
            i += 1;
            let name_start = i;
            while i < bytes.len() && bytes[i] != b']' {
                i += 1;
            }
            if i >= bytes.len() {
                return None;
            }
            let name = sql[name_start..i].trim();
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        }
        b if b.is_ascii_alphabetic() || b == b'_' => {
            let name_start = i;
            while i < bytes.len() && is_ident_char(bytes[i]) {
                i += 1;
            }
            let name = sql[name_start..i].trim();
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        }
        _ => None,
    }
}

fn is_dotted_identifier_suffix(sql: &str, keyword_start: usize) -> bool {
    let mut index = keyword_start;
    while index > 0 {
        index -= 1;
        match sql.as_bytes()[index] {
            byte if byte.is_ascii_whitespace() => {}
            b'.' => return true,
            _ => return false,
        }
    }
    false
}

fn is_sqlite_trigger_body_end(
    keyword: &str,
    in_trigger_body: bool,
    trigger_body_stmt_start: bool,
    sql: &str,
    keyword_start: usize,
) -> bool {
    in_trigger_body
        && trigger_body_stmt_start
        && keyword.eq_ignore_ascii_case("END")
        && !is_dotted_identifier_suffix(sql, keyword_start)
}

pub(in crate::adapters::sqlite::sqlite3) fn try_split_sqlite_statements(
    sql: &str,
) -> Result<Vec<&str>, DbOperationError> {
    reject_sqlite_meta_commands(sql)?;

    let bytes = sql.as_bytes();
    let mut statements = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut in_trigger_body = false;
    let mut trigger_body_stmt_start = false;

    while i < bytes.len() {
        match bytes[i] {
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    i += 2;
                }
            }
            b'\'' | b'"' | b'`' => {
                i = skip_quoted(bytes, i, bytes[i]);
            }
            b'[' => {
                i = skip_bracket_quoted(bytes, i);
            }
            b';' if in_trigger_body => {
                trigger_body_stmt_start = true;
                i += 1;
            }
            b';' => {
                let statement = sql[start..i].trim();
                if !statement.is_empty() {
                    statements.push(statement);
                }
                i += 1;
                start = i;
                in_trigger_body = false;
                trigger_body_stmt_start = false;
            }
            _ => {
                if bytes[i].is_ascii_alphabetic()
                    && is_create_trigger_prefix(&sql[start..])
                    && let Some((keyword, kw_end)) = next_keyword_from(sql, i)
                {
                    if keyword.eq_ignore_ascii_case("BEGIN") {
                        if in_trigger_body {
                            trigger_body_stmt_start = false;
                        } else {
                            in_trigger_body = true;
                            trigger_body_stmt_start = true;
                        }
                    } else if is_sqlite_trigger_body_end(
                        keyword,
                        in_trigger_body,
                        trigger_body_stmt_start,
                        sql,
                        i,
                    ) {
                        in_trigger_body = false;
                        trigger_body_stmt_start = false;
                    } else if in_trigger_body {
                        trigger_body_stmt_start = false;
                    }
                    i = kw_end;
                    continue;
                }
                i += 1;
            }
        }
    }

    let tail = sql[start..].trim();
    if !tail.is_empty() {
        if in_trigger_body {
            return Err(DbOperationError::QueryFailed(
                "Unclosed CREATE TRIGGER body".to_string(),
            ));
        }
        if is_create_trigger_prefix(tail) && !contains_keyword(tail, "BEGIN") {
            return Err(DbOperationError::QueryFailed(
                "Incomplete CREATE TRIGGER statement".to_string(),
            ));
        }
        statements.push(tail);
    }

    Ok(statements)
}

fn reject_sqlite_meta_commands(sql: &str) -> Result<(), DbOperationError> {
    if contains_sqlite_meta_command(sql) {
        return Err(DbOperationError::UnsupportedOperation(
            "SQLite dot commands are not supported".to_string(),
        ));
    }
    Ok(())
}

fn contains_sqlite_meta_command(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut i = 0;
    let mut line_start = true;

    while i < bytes.len() {
        match bytes[i] {
            b'\n' | b'\r' => {
                line_start = true;
                i += 1;
            }
            b' ' | b'\t' if line_start => i += 1,
            b'.' if line_start => return true,
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                line_start = false;
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    if bytes[i] == b'\n' || bytes[i] == b'\r' {
                        line_start = true;
                    }
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    i += 2;
                }
            }
            b'\'' | b'"' | b'`' => {
                line_start = false;
                i = skip_quoted(bytes, i, bytes[i]);
            }
            b'[' => {
                line_start = false;
                i = skip_bracket_quoted(bytes, i);
            }
            _ => {
                line_start = false;
                i += 1;
            }
        }
    }

    false
}

fn is_transaction_control(statement: &str) -> bool {
    matches!(
        first_keyword(statement).to_ascii_uppercase().as_str(),
        "BEGIN" | "COMMIT" | "END" | "ROLLBACK" | "SAVEPOINT" | "RELEASE"
    )
}

fn is_write_statement(statement: &str) -> bool {
    if is_dml_statement(statement) {
        return true;
    }
    matches!(
        first_keyword(statement).to_ascii_uppercase().as_str(),
        "CREATE" | "ALTER" | "DROP" | "TRUNCATE"
    )
}

pub(in crate::adapters::sqlite::sqlite3) fn is_sqlite_rerunnable_export_query(
    query: &str,
) -> Result<bool, DbOperationError> {
    let statements = try_split_sqlite_statements(query)?;
    Ok(statements.len() == 1
        && statements
            .iter()
            .all(|statement| is_sqlite_rerunnable_export_statement(statement)))
}

fn is_sqlite_rerunnable_export_statement(statement: &str) -> bool {
    if is_write_statement(statement)
        || is_dml_statement(statement)
        || is_transaction_control(statement)
    {
        return false;
    }
    match first_keyword(statement).to_ascii_uppercase().as_str() {
        "SELECT" | "EXPLAIN" | "VALUES" => true,
        "WITH" => !is_dml_statement(statement),
        "PRAGMA" => is_read_only_sqlite_export_pragma(statement),
        _ => false,
    }
}

fn sqlite_pragma_name(statement: &str) -> Option<String> {
    let (_, pragma_end) = next_keyword_from(statement, 0)?;
    let tail = statement.get(pragma_end..)?.trim_start();
    let name: String = tail
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(
            name.rsplit('.')
                .next()
                .unwrap_or(name.as_str())
                .to_ascii_lowercase(),
        )
    }
}

fn is_read_only_parameterized_pragma(name: &str) -> bool {
    matches!(
        name,
        "table_info"
            | "table_xinfo"
            | "index_info"
            | "index_xinfo"
            | "index_list"
            | "foreign_key_list"
            | "database_list"
            | "table_list"
            | "pragma_list"
            | "function_list"
            | "module_list"
            | "collation_list"
            | "integrity_check"
            | "quick_check"
            | "column_info"
    )
}

fn is_read_only_sqlite_export_pragma(statement: &str) -> bool {
    let Some(name) = sqlite_pragma_name(statement) else {
        return false;
    };
    let has_assignment = statement.contains('=');
    let has_parenthesized_value = statement.contains('(');
    let side_effect_without_assignment = (has_parenthesized_value
        && !is_read_only_parameterized_pragma(&name))
        || matches!(
            name.as_str(),
            "optimize" | "incremental_vacuum" | "wal_checkpoint"
        );
    !has_assignment && !side_effect_without_assignment
}

pub(in crate::adapters::sqlite::sqlite3) fn sqlite_export_not_rerunnable_error() -> DbOperationError
{
    DbOperationError::UnsupportedOperation(
        "Cannot re-execute this query for CSV export because it contains write or DDL statements"
            .to_string(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::adapters::sqlite::sqlite3) enum SqliteWrapMode {
    None,
    BeginCommit,
}

fn needs_write_batch_wrap(statements: &[&str]) -> bool {
    statements.len() > 1 && statements.iter().any(|stmt| is_write_statement(stmt))
}

fn rollback_has_to_clause(statement: &str) -> bool {
    if !first_keyword(statement).eq_ignore_ascii_case("ROLLBACK") {
        return false;
    }
    let mut offset = 0;
    while let Some((keyword, end)) = next_keyword_from(statement, offset) {
        if keyword.eq_ignore_ascii_case("TO") {
            return true;
        }
        offset = end;
    }
    false
}

pub(super) fn rollback_to_target(statement: &str) -> Option<&str> {
    let (_, first_end) = next_keyword_from(statement, 0)?;
    if !first_keyword(statement).eq_ignore_ascii_case("ROLLBACK") {
        return None;
    }
    let (second, second_end) = next_keyword_from(statement, first_end)?;
    if second.eq_ignore_ascii_case("TRANSACTION") {
        let (third, third_end) = next_keyword_from(statement, second_end)?;
        if !third.eq_ignore_ascii_case("TO") {
            return None;
        }
        let (fourth, fourth_end) = identifier_token_from(statement, third_end)?;
        if fourth.eq_ignore_ascii_case("SAVEPOINT") {
            identifier_token_from(statement, fourth_end).map(|(name, _)| name)
        } else {
            identifier_token_from(statement, third_end).map(|(name, _)| name)
        }
    } else if second.eq_ignore_ascii_case("TO") {
        let (third, third_end) = identifier_token_from(statement, second_end)?;
        if third.eq_ignore_ascii_case("SAVEPOINT") {
            identifier_token_from(statement, third_end).map(|(name, _)| name)
        } else {
            identifier_token_from(statement, second_end).map(|(name, _)| name)
        }
    } else {
        None
    }
}

pub(super) fn savepoint_target(statement: &str) -> Option<&str> {
    let (_, first_end) = next_keyword_from(statement, 0)?;
    let (target, target_end) = identifier_token_from(statement, first_end)?;
    if first_keyword(statement).eq_ignore_ascii_case("RELEASE")
        && target.eq_ignore_ascii_case("SAVEPOINT")
    {
        identifier_token_from(statement, target_end).map(|(name, _)| name)
    } else {
        Some(target)
    }
}

fn identifier_token_from(sql: &str, mut i: usize) -> Option<(&str, usize)> {
    let bytes = sql.as_bytes();
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }

    let start = i;
    let end = match bytes[i] {
        b'"' | b'\'' | b'`' => skip_quoted(bytes, i, bytes[i]),
        b'[' => skip_bracket_quoted(bytes, i),
        _ => {
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && bytes[i] != b';'
                && bytes[i] != b','
            {
                i += 1;
            }
            if i == start {
                return None;
            }
            i
        }
    };

    Some((&sql[start..end], end))
}

pub(super) fn is_rollback_to(statement: &str) -> bool {
    rollback_to_target(statement).is_some() || rollback_has_to_clause(statement)
}

pub(in crate::adapters::sqlite::sqlite3) fn sqlite_wrap_mode(
    query: &str,
) -> Result<SqliteWrapMode, DbOperationError> {
    let statements = try_split_sqlite_statements(query)?;
    if !needs_write_batch_wrap(&statements) {
        return Ok(SqliteWrapMode::None);
    }

    if statements.iter().any(|stmt| is_transaction_control(stmt)) {
        return Ok(SqliteWrapMode::None);
    }

    Ok(SqliteWrapMode::BeginCommit)
}

fn sqlite_transaction_block(query: &str) -> String {
    let trimmed = query.trim_end().trim_end_matches(';').trim_end();
    format!("BEGIN;\n{trimmed}\n;\nCOMMIT")
}

fn sqlite_execution_query(query: &str) -> Result<Cow<'_, str>, DbOperationError> {
    Ok(match sqlite_wrap_mode(query)? {
        SqliteWrapMode::BeginCommit => Cow::Owned(sqlite_transaction_block(query)),
        SqliteWrapMode::None => Cow::Borrowed(query),
    })
}

pub(in crate::adapters::sqlite::sqlite3) fn sqlite_probe_marker() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "__sabiql_sqlite_probe_{}_{}_{}",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

pub(super) fn sqlite_probe_columns(marker: &str) -> (String, String) {
    (format!("{marker}_stmt"), format!("{marker}_changes"))
}

fn sqlite_changes_probe(marker: &str, index: usize) -> String {
    let (stmt_col, changes_col) = sqlite_probe_columns(marker);
    format!("SELECT {index} AS \"{stmt_col}\", changes() AS \"{changes_col}\"")
}

pub(super) fn sqlite_result_probe_columns(marker: &str) -> (String, String) {
    (
        format!("{marker}_result_stmt"),
        format!("{marker}_result_marker"),
    )
}

fn sqlite_result_probe(marker: &str, index: usize) -> String {
    let (stmt_col, marker_col) = sqlite_result_probe_columns(marker);
    format!("SELECT {index} AS \"{stmt_col}\", '{marker}' AS \"{marker_col}\"")
}

pub(in crate::adapters::sqlite::sqlite3) fn sqlite_adhoc_execution_query(
    query: &str,
    marker: &str,
) -> Result<String, DbOperationError> {
    let statements = try_split_sqlite_statements(query)?;
    if statements.is_empty() {
        return Ok(query.to_string());
    }

    let wrap_mode = sqlite_wrap_mode(query)?;
    let mut parts = Vec::with_capacity(statements.len() * 2 + 2);
    if matches!(wrap_mode, SqliteWrapMode::BeginCommit) {
        parts.push("BEGIN".to_string());
    }
    for (index, statement) in statements.iter().enumerate() {
        parts.push((*statement).to_string());
        if is_dml_statement(statement) {
            parts.push(sqlite_changes_probe(marker, index));
        }
        if statement_emits_result_set(statement) {
            parts.push(sqlite_result_probe(marker, index));
        }
    }
    if matches!(wrap_mode, SqliteWrapMode::BeginCommit) {
        parts.push("COMMIT".to_string());
    }
    Ok(parts.join("\n;\n"))
}

pub(in crate::adapters::sqlite::sqlite3) fn append_changes_query(
    query: &str,
) -> Result<String, DbOperationError> {
    let body = sqlite_execution_query(query)?.trim_end().to_string();
    // The standalone separator also terminates a trailing line comment before
    // appending the changes() probe.
    Ok(format!("{body}\n;\nSELECT changes() AS affected_rows;"))
}

pub(super) fn dml_keyword(statement: &str) -> Option<&'static str> {
    let keyword = first_keyword(statement);
    if keyword.eq_ignore_ascii_case("INSERT") {
        return Some("INSERT");
    }
    if keyword.eq_ignore_ascii_case("REPLACE") {
        return Some("INSERT");
    }
    if keyword.eq_ignore_ascii_case("UPDATE") {
        return Some("UPDATE");
    }
    if keyword.eq_ignore_ascii_case("DELETE") {
        return Some("DELETE");
    }
    if !keyword.eq_ignore_ascii_case("WITH") {
        return None;
    }

    let mut offset = 0;
    while let Some((keyword, end)) = next_keyword_from(statement, offset) {
        if keyword.eq_ignore_ascii_case("INSERT") {
            return Some("INSERT");
        }
        if keyword.eq_ignore_ascii_case("REPLACE") {
            return Some("INSERT");
        }
        if keyword.eq_ignore_ascii_case("UPDATE") {
            return Some("UPDATE");
        }
        if keyword.eq_ignore_ascii_case("DELETE") {
            return Some("DELETE");
        }
        offset = end;
    }
    None
}

fn is_dml_statement(statement: &str) -> bool {
    dml_keyword(statement).is_some()
}

fn statement_emits_result_set(statement: &str) -> bool {
    let keyword = first_keyword(statement);
    if keyword.eq_ignore_ascii_case("SELECT")
        || keyword.eq_ignore_ascii_case("PRAGMA")
        || keyword.eq_ignore_ascii_case("EXPLAIN")
        || keyword.eq_ignore_ascii_case("VALUES")
    {
        return true;
    }
    if is_dml_statement(statement) {
        return contains_keyword(statement, "RETURNING");
    }
    keyword.eq_ignore_ascii_case("WITH")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ports::outbound::DbOperationError;

    #[test]
    fn split_sqlite_statements_ignores_semicolons_in_literals_and_comments() {
        let statements = try_split_sqlite_statements(
            "INSERT INTO logs(message) VALUES ('a;b'); -- ; ignored\nSELECT ';' AS value;",
        )
        .unwrap();

        assert_eq!(
            statements,
            vec![
                "INSERT INTO logs(message) VALUES ('a;b')",
                "-- ; ignored\nSELECT ';' AS value"
            ]
        );
    }

    #[test]
    fn split_sqlite_statements_rejects_dot_commands() {
        let error = try_split_sqlite_statements("SELECT 1;\n.shell echo unsafe").unwrap_err();

        assert!(matches!(error, DbOperationError::UnsupportedOperation(_)));
    }

    #[test]
    fn split_sqlite_statements_allows_dot_at_line_start_inside_literal() {
        let statements =
            try_split_sqlite_statements("SELECT '.shell echo safe\n.read file';").unwrap();

        assert_eq!(statements, vec!["SELECT '.shell echo safe\n.read file'"]);
    }

    #[test]
    fn split_sqlite_statements_keeps_create_trigger_body_together() {
        let trigger = "\
CREATE TRIGGER agent_messages_fts_ai AFTER INSERT ON agent_messages BEGIN
    INSERT INTO agent_messages_fts(rowid, role, content)
    VALUES (new.id, new.role, new.content);
END";
        let sql = format!("{trigger}; SELECT 1 AS value;");

        let statements = try_split_sqlite_statements(&sql).unwrap();

        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0], trigger);
        assert_eq!(statements[1], "SELECT 1 AS value");
    }

    #[test]
    fn split_sqlite_statements_keeps_create_trigger_with_dotted_end_reference() {
        let trigger = "\
CREATE TRIGGER sync_end AFTER UPDATE ON events BEGIN
    UPDATE counters SET end_value = new.end WHERE id = new.id;
    INSERT INTO audit(event_id, end_value) VALUES (new.id, new.end);
END";
        let sql = format!("{trigger}; SELECT 1 AS value;");

        let statements = try_split_sqlite_statements(&sql).unwrap();

        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0], trigger);
        assert_eq!(statements[1], "SELECT 1 AS value");
    }

    #[test]
    fn split_sqlite_statements_keeps_create_trigger_with_case_end_expression() {
        let trigger = "\
CREATE TRIGGER normalize_events AFTER UPDATE ON events BEGIN
    UPDATE counters
    SET end_value = CASE WHEN new.end > 0 THEN new.end ELSE old.end END
    WHERE id = new.id;
    INSERT INTO audit(event_id) VALUES (new.id);
END";
        let sql = format!("{trigger}; SELECT 1 AS value;");

        let statements = try_split_sqlite_statements(&sql).unwrap();

        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0], trigger);
        assert_eq!(statements[1], "SELECT 1 AS value");
    }

    #[test]
    fn adhoc_execution_query_does_not_insert_probes_when_trigger_references_new_end() {
        let trigger = "\
CREATE TRIGGER sync_end AFTER UPDATE ON events BEGIN
    UPDATE counters SET end_value = new.end WHERE id = new.id;
    INSERT INTO audit(event_id, end_value) VALUES (new.id, new.end);
END";
        let marker = "probe_marker";

        let execution_query = sqlite_adhoc_execution_query(trigger, marker).unwrap();

        assert!(!execution_query.contains(marker));
        assert_eq!(execution_query, trigger);
    }

    #[test]
    fn split_sqlite_statements_rejects_unclosed_create_trigger_body() {
        let error = try_split_sqlite_statements(
            "CREATE TRIGGER t AFTER INSERT ON users BEGIN INSERT INTO logs(id) VALUES (1);",
        )
        .unwrap_err();

        assert!(matches!(error, DbOperationError::QueryFailed(_)));
    }

    #[test]
    fn split_sqlite_statements_rejects_incomplete_create_trigger_without_begin() {
        let error =
            try_split_sqlite_statements("CREATE TRIGGER t AFTER INSERT ON users").unwrap_err();

        assert!(matches!(error, DbOperationError::QueryFailed(_)));
    }

    #[test]
    fn adhoc_execution_query_does_not_insert_probes_inside_create_trigger() {
        let trigger = "\
CREATE TRIGGER agent_messages_fts_ai AFTER INSERT ON agent_messages BEGIN
    INSERT INTO agent_messages_fts(rowid, role, content)
    VALUES (new.id, new.role, new.content);
END";
        let marker = "probe_marker";

        let execution_query = sqlite_adhoc_execution_query(trigger, marker).unwrap();

        assert!(!execution_query.contains(marker));
        assert_eq!(execution_query, trigger);
    }

    #[test]
    fn append_changes_wraps_multi_statement_write_without_explicit_transaction() {
        let query = "INSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2);";

        let wrapped = append_changes_query(query).unwrap();

        assert_eq!(
            wrapped,
            "BEGIN;\nINSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2)\n;\nCOMMIT\n;\nSELECT changes() AS affected_rows;"
        );
    }

    #[test]
    fn append_changes_wraps_multi_statement_replace_without_explicit_transaction() {
        let query = "REPLACE INTO users(id) VALUES (1); SELECT * FROM missing";

        let wrapped = append_changes_query(query).unwrap();

        assert_eq!(
            wrapped,
            "BEGIN;\nREPLACE INTO users(id) VALUES (1); SELECT * FROM missing\n;\nCOMMIT\n;\nSELECT changes() AS affected_rows;"
        );
    }

    #[test]
    fn append_changes_wraps_multi_statement_with_write_without_explicit_transaction() {
        let query = "WITH payload(id) AS (VALUES (1)) INSERT INTO users(id) SELECT id FROM payload; SELECT * FROM missing";

        let wrapped = append_changes_query(query).unwrap();

        assert_eq!(
            wrapped,
            "BEGIN;\nWITH payload(id) AS (VALUES (1)) INSERT INTO users(id) SELECT id FROM payload; SELECT * FROM missing\n;\nCOMMIT\n;\nSELECT changes() AS affected_rows;"
        );
    }

    #[test]
    fn append_changes_keeps_explicit_begin_commit_transaction_control() {
        let query = "BEGIN; INSERT INTO users(id) VALUES (1); COMMIT";

        let wrapped = append_changes_query(query).unwrap();

        assert_eq!(
            wrapped,
            "BEGIN; INSERT INTO users(id) VALUES (1); COMMIT\n;\nSELECT changes() AS affected_rows;"
        );
    }

    #[test]
    fn append_changes_keeps_explicit_begin_end_transaction_control() {
        let query = "BEGIN; INSERT INTO users(id) VALUES (1); END";

        let wrapped = append_changes_query(query).unwrap();

        assert_eq!(
            wrapped,
            "BEGIN; INSERT INTO users(id) VALUES (1); END\n;\nSELECT changes() AS affected_rows;"
        );
    }

    mod wrap_mode {
        use super::*;

        #[test]
        fn autocommit_multi_write_uses_begin_commit() {
            assert_eq!(
                sqlite_wrap_mode(
                    "INSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2)"
                )
                .unwrap(),
                SqliteWrapMode::BeginCommit
            );
        }

        #[test]
        fn explicit_begin_is_not_wrapped() {
            assert_eq!(
                sqlite_wrap_mode("BEGIN; INSERT INTO users(id) VALUES (1); COMMIT").unwrap(),
                SqliteWrapMode::None
            );
        }

        #[test]
        fn top_level_savepoint_multi_write_is_not_wrapped() {
            assert_eq!(
                sqlite_wrap_mode(
                    "SAVEPOINT user_sp; INSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2)"
                )
                .unwrap(),
                SqliteWrapMode::None
            );
        }

        #[test]
        fn mid_batch_savepoint_is_not_wrapped() {
            assert_eq!(
                sqlite_wrap_mode(
                    "INSERT INTO users(id) VALUES (1); SAVEPOINT sp; INSERT INTO users(id) VALUES (2)"
                )
                .unwrap(),
                SqliteWrapMode::None
            );
        }
    }

    #[test]
    fn export_guard_rejects_non_rerunnable_sql() {
        for sql in [
            "SELECT 1; SELECT 2",
            "WITH payload(id) AS (VALUES (1)) INSERT INTO users(id) SELECT id FROM payload",
            "PRAGMA foreign_keys=OFF",
            "PRAGMA wal_checkpoint(TRUNCATE)",
        ] {
            assert!(!is_sqlite_rerunnable_export_query(sql).unwrap(), "{sql}");
        }
    }

    #[test]
    fn export_guard_allows_read_only_sql() {
        for sql in ["SELECT 1", "PRAGMA table_info(users)"] {
            assert!(is_sqlite_rerunnable_export_query(sql).unwrap(), "{sql}");
        }
    }

    #[test]
    fn create_virtual_table_prefix_requires_keyword_sequence() {
        assert!(is_create_virtual_table_prefix(
            "CREATE VIRTUAL TABLE notes_fts USING fts5(body);"
        ));
        assert!(!is_create_virtual_table_prefix(
            "CREATE TABLE docs(body TEXT DEFAULT 'create virtual table');"
        ));
    }

    #[test]
    fn virtual_table_module_name_skips_quoted_table_name() {
        assert_eq!(
            virtual_table_module_name(r#"CREATE VIRTUAL TABLE "using" USING fts5(body);"#),
            Some("fts5".to_string())
        );
    }

    #[test]
    fn virtual_table_module_name_reads_double_quoted_module() {
        assert_eq!(
            virtual_table_module_name(r#"CREATE VIRTUAL TABLE notes USING "fts5"(body);"#),
            Some("fts5".to_string())
        );
    }

    #[test]
    fn virtual_table_module_name_rejects_unclosed_bracket_module() {
        assert_eq!(
            virtual_table_module_name("CREATE VIRTUAL TABLE notes USING [fts5(body);"),
            None
        );
    }
}
