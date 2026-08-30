use super::splitter::{
    first_sqlite_keyword, keywords_with_depth, statement_keyword, top_level_keywords,
};
use crate::sql_lex::{
    advance_single_quote, skip_block_comment, skip_double_quoted_identifier, skip_line_comment,
    skip_sqlite_quoted_identifier,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteStatementClassification {
    ReadOnly,
    TransactionalWrite,
    SessionSideEffect,
    TransactionIncompatible,
    TransactionControl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlitePragma {
    pub name: String,
    pub has_value: bool,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteTransactionPolicy {
    AutoWrap,
    NotNeeded,
    UserManaged,
    IncompatibleStatement,
    ClassificationMismatch,
}

impl SqliteTransactionPolicy {
    pub fn requires_acknowledgement(self) -> bool {
        matches!(self, Self::IncompatibleStatement)
    }

    pub fn is_invalid(self) -> bool {
        matches!(self, Self::ClassificationMismatch)
    }
}

pub fn sqlite_transaction_policy_for_classifications(
    statement_count: usize,
    classifications: &[SqliteStatementClassification],
) -> SqliteTransactionPolicy {
    if statement_count != classifications.len() {
        return SqliteTransactionPolicy::ClassificationMismatch;
    }
    if statement_count < 2 {
        return SqliteTransactionPolicy::NotNeeded;
    }
    if classifications.iter().any(|classification| {
        matches!(
            classification,
            SqliteStatementClassification::TransactionControl
        )
    }) {
        return SqliteTransactionPolicy::UserManaged;
    }
    if classifications.iter().any(|classification| {
        matches!(
            classification,
            SqliteStatementClassification::SessionSideEffect
                | SqliteStatementClassification::TransactionIncompatible
        )
    }) {
        return SqliteTransactionPolicy::IncompatibleStatement;
    }
    if classifications.iter().any(|classification| {
        matches!(
            classification,
            SqliteStatementClassification::TransactionalWrite
        )
    }) {
        SqliteTransactionPolicy::AutoWrap
    } else {
        SqliteTransactionPolicy::NotNeeded
    }
}

pub fn sqlite_statement_classification(statement: &str) -> SqliteStatementClassification {
    if is_transaction_control(statement) {
        return SqliteStatementClassification::TransactionControl;
    }
    if is_transaction_incompatible(statement) {
        return SqliteStatementClassification::TransactionIncompatible;
    }
    if is_transactional_pragma_write(statement) {
        return SqliteStatementClassification::TransactionalWrite;
    }
    if is_session_pragma_side_effect(statement) {
        return SqliteStatementClassification::SessionSideEffect;
    }
    if has_data_modifying_cte(statement) {
        return SqliteStatementClassification::TransactionalWrite;
    }
    let first_keyword = first_sqlite_keyword(statement);
    if first_keyword.as_deref() == Some("EXPLAIN") {
        return match explain_analyze_statement_keyword(statement).as_deref() {
            Some("INSERT" | "UPDATE" | "DELETE" | "CREATE" | "ALTER" | "DROP" | "TRUNCATE") => {
                SqliteStatementClassification::TransactionalWrite
            }
            _ => SqliteStatementClassification::ReadOnly,
        };
    }
    match statement_keyword(statement).as_deref() {
        Some("ATTACH" | "DETACH") => SqliteStatementClassification::SessionSideEffect,
        Some(
            "ANALYZE" | "REINDEX" | "INSERT" | "UPDATE" | "DELETE" | "CREATE" | "ALTER" | "DROP"
            | "TRUNCATE" | "REPLACE",
        ) => SqliteStatementClassification::TransactionalWrite,
        _ => SqliteStatementClassification::ReadOnly,
    }
}

fn explain_analyze_statement_keyword(statement: &str) -> Option<String> {
    let keywords = keywords_with_depth(statement);
    if keywords
        .first()
        .map(|(keyword, depth)| (keyword.as_str(), *depth))
        != Some(("EXPLAIN", 0))
    {
        return None;
    }
    let analyze_index =
        keywords
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(index, (keyword, depth))| {
                (*depth <= 1 && keyword == "ANALYZE").then_some(index)
            })?;
    keywords
        .iter()
        .skip(analyze_index + 1)
        .find(|(_, depth)| *depth == 0)
        .map(|(keyword, _)| keyword.clone())
}

fn has_data_modifying_cte(statement: &str) -> bool {
    if first_sqlite_keyword(statement).as_deref() != Some("WITH") {
        return false;
    }
    keywords_with_depth(statement)
        .into_iter()
        .any(|(keyword, depth)| {
            depth > 0 && matches!(keyword.as_str(), "INSERT" | "UPDATE" | "DELETE")
        })
}

fn is_transaction_control(statement: &str) -> bool {
    let keywords = top_level_keywords(statement);
    matches!(
        keywords.first().map(String::as_str),
        Some("BEGIN" | "COMMIT" | "ROLLBACK" | "SAVEPOINT" | "RELEASE")
    ) || matches!(
        keywords.as_slice(),
        [first, second, ..] if first == "START" && second == "TRANSACTION"
    )
}

fn is_transaction_incompatible(statement: &str) -> bool {
    if first_sqlite_keyword(statement).as_deref() == Some("VACUUM") {
        return true;
    }
    let Some(pragma) = parse_sqlite_pragma(statement) else {
        return false;
    };
    matches!(
        pragma.name.as_str(),
        "journal_mode" | "foreign_keys" | "synchronous"
    ) && pragma.has_value
}

fn is_transactional_pragma_write(statement: &str) -> bool {
    let Some(pragma) = parse_sqlite_pragma(statement) else {
        return false;
    };
    matches!(pragma.name.as_str(), "application_id" | "user_version") && pragma.has_value
}

fn is_session_pragma_side_effect(statement: &str) -> bool {
    let Some(pragma) = parse_sqlite_pragma(statement) else {
        return false;
    };
    (pragma.has_value && !is_read_only_parameterized_pragma(&pragma.name))
        || matches!(
            pragma.name.as_str(),
            "optimize" | "incremental_vacuum" | "wal_checkpoint"
        )
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

fn trim_sql_prefix(mut sql: &str) -> &str {
    loop {
        let trimmed = sql.trim_start();
        if let Some(comment) = trimmed.strip_prefix("--") {
            sql = comment.find('\n').map_or("", |index| &comment[index + 1..]);
            continue;
        }
        if let Some(comment) = trimmed.strip_prefix("/*") {
            sql = comment.find("*/").map_or("", |index| &comment[index + 2..]);
            continue;
        }
        return trimmed;
    }
}

pub fn parse_sqlite_pragma(statement: &str) -> Option<SqlitePragma> {
    let trimmed = trim_sql_prefix(statement);
    if !trimmed.get(..6)?.eq_ignore_ascii_case("PRAGMA") {
        return None;
    }
    let tail = trim_sql_prefix(trimmed.get(6..)?);
    let (first_name, rest) = pragma_identifier_and_tail(tail)?;
    let rest = trim_sql_prefix(rest);
    let (name, rest) = if let Some(rest) = rest.strip_prefix('.') {
        let (name, rest) = pragma_identifier_and_tail(trim_sql_prefix(rest))?;
        (name, rest)
    } else {
        (first_name, rest)
    };
    let rest = trim_sql_prefix(rest);
    let has_value = rest.starts_with('=') || rest.starts_with('(');
    let value = pragma_value(rest);
    Some(SqlitePragma {
        name: name.to_ascii_lowercase(),
        has_value,
        value,
    })
}

fn pragma_value(tail: &str) -> Option<String> {
    let (value, stop_at_close) = if let Some(value) = tail.strip_prefix('=') {
        (value, false)
    } else {
        let value = tail.strip_prefix('(')?;
        (value, true)
    };
    let chars: Vec<(usize, char)> = value.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (byte_pos, ch) = chars[i];
        if let Some(next) = skip_line_comment(&chars, i, ch) {
            i = next;
            continue;
        }
        if let Some(next) = skip_block_comment(&chars, i, ch) {
            if next > chars.len() {
                return None;
            }
            i = next;
            continue;
        }
        if matches!(ch, '\'' | '"' | '`' | '[') {
            let next = pragma_quoted_value_end(&chars, i, ch)?;
            let content_start = chars.get(i + 1)?.0;
            let content_end = chars.get(next.checked_sub(1)?)?.0;
            return Some(value[content_start..content_end].to_ascii_lowercase());
        }
        if stop_at_close && ch == ')' {
            return None;
        }
        let has_numeric_sign = matches!(ch, '+' | '-')
            && chars
                .get(i + 1)
                .is_some_and(|(_, next)| next.is_ascii_digit());
        if has_numeric_sign || ch.is_alphanumeric() || ch == '_' {
            let start = byte_pos;
            let mut end = if has_numeric_sign { i + 2 } else { i + 1 };
            while end < chars.len() && (chars[end].1.is_alphanumeric() || chars[end].1 == '_') {
                end += 1;
            }
            let end_byte = chars.get(end).map_or(value.len(), |(pos, _)| *pos);
            return Some(value[start..end_byte].to_ascii_lowercase());
        }
        i += 1;
    }
    None
}

fn pragma_quoted_value_end(chars: &[(usize, char)], i: usize, ch: char) -> Option<usize> {
    let end = match ch {
        '\'' => {
            let mut in_string = false;
            let mut cursor = i;
            while cursor < chars.len() {
                if chars[cursor].1 == '\'' {
                    cursor = advance_single_quote(chars, cursor, chars[cursor].1, &mut in_string)?;
                    if !in_string {
                        return Some(cursor);
                    }
                } else {
                    cursor += 1;
                }
            }
            return None;
        }
        '"' => skip_double_quoted_identifier(chars, i, ch)?,
        '`' | '[' => skip_sqlite_quoted_identifier(chars, i, ch)?,
        _ => return None,
    };
    let close = if ch == '[' { ']' } else { ch };
    (end <= chars.len() && end > i && chars.get(end - 1).is_some_and(|(_, last)| *last == close))
        .then_some(end)
}

fn pragma_identifier_and_tail(sql: &str) -> Option<(&str, &str)> {
    let (name, rest) = match sql.as_bytes().first()? {
        b'"' | b'\'' | b'`' => {
            let quote = sql.as_bytes()[0] as char;
            let end = sql[1..].find(quote)? + 1;
            (sql.get(1..end)?, sql.get(end + 1..)?)
        }
        b'[' => {
            let end = sql.find(']')?;
            (sql.get(1..end)?, sql.get(end + 1..)?)
        }
        _ => {
            let end = sql
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .unwrap_or(sql.len());
            (sql.get(..end)?, sql.get(end..)?)
        }
    };
    (!name.is_empty()).then_some((name, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_pragma_changes_and_queries() {
        assert_eq!(
            sqlite_statement_classification("PRAGMA journal_mode"),
            SqliteStatementClassification::ReadOnly
        );
        assert_eq!(
            sqlite_statement_classification("PRAGMA main.journal_mode = WAL"),
            SqliteStatementClassification::TransactionIncompatible
        );
        assert_eq!(
            sqlite_statement_classification("PRAGMA user_version = 42"),
            SqliteStatementClassification::TransactionalWrite
        );
    }

    #[test]
    fn preserves_pragma_classification_after_value_comments() {
        assert_eq!(
            sqlite_statement_classification("PRAGMA journal_mode = /* comment */ WAL"),
            SqliteStatementClassification::TransactionIncompatible
        );
        assert_eq!(
            sqlite_statement_classification("PRAGMA application_id(/* comment */ 7)"),
            SqliteStatementClassification::TransactionalWrite
        );
    }

    #[test]
    fn classifies_cte_writes_and_transaction_control() {
        assert_eq!(
            sqlite_statement_classification(
                "WITH payload(id) AS (VALUES (1)) INSERT INTO users(id) SELECT id FROM payload"
            ),
            SqliteStatementClassification::TransactionalWrite
        );
        assert_eq!(
            sqlite_statement_classification(
                "WITH \"payload\"(id) AS (VALUES (1)) SELECT id FROM \"payload\""
            ),
            SqliteStatementClassification::ReadOnly
        );
        assert_eq!(
            sqlite_statement_classification("EXPLAIN ANALYZE UPDATE users SET name = 'a'"),
            SqliteStatementClassification::TransactionalWrite
        );
        assert_eq!(
            sqlite_statement_classification("EXPLAIN (ANALYZE) UPDATE users SET name = 'a'"),
            SqliteStatementClassification::TransactionalWrite
        );
        assert_eq!(
            sqlite_statement_classification(
                "EXPLAIN (ANALYZE, VERBOSE) DELETE FROM users WHERE id = 1"
            ),
            SqliteStatementClassification::TransactionalWrite
        );
        assert_eq!(
            sqlite_statement_classification("EXPLAIN QUERY PLAN UPDATE users SET name = 'a'"),
            SqliteStatementClassification::ReadOnly
        );
        assert_eq!(
            sqlite_statement_classification(
                "WITH payload(id) AS (VALUES (1)) REPLACE INTO users(id) SELECT id FROM payload"
            ),
            SqliteStatementClassification::TransactionalWrite
        );
        assert_eq!(
            sqlite_statement_classification("START TRANSACTION"),
            SqliteStatementClassification::TransactionControl
        );
        assert_eq!(
            sqlite_statement_classification(
                "WITH changed AS (UPDATE users SET name = 'a' RETURNING *) SELECT * FROM changed"
            ),
            SqliteStatementClassification::TransactionalWrite
        );
    }

    #[test]
    fn cte_replace_enters_the_automatic_transaction_policy() {
        let classification = sqlite_statement_classification(
            "WITH payload(id) AS (VALUES (1)) REPLACE INTO users(id) SELECT id FROM payload",
        );

        assert_eq!(
            classification,
            SqliteStatementClassification::TransactionalWrite
        );
        assert_eq!(
            sqlite_transaction_policy_for_classifications(
                2,
                &[classification, SqliteStatementClassification::ReadOnly],
            ),
            SqliteTransactionPolicy::AutoWrap
        );
    }

    #[test]
    fn transaction_policy_rejects_mismatched_classifications() {
        assert_eq!(
            sqlite_transaction_policy_for_classifications(1, &[]),
            SqliteTransactionPolicy::ClassificationMismatch
        );
    }

    #[test]
    fn parses_qualified_quoted_pragma_names() {
        let pragma = parse_sqlite_pragma("PRAGMA [main].[application_id](7)").unwrap();

        assert_eq!(pragma.name, "application_id");
        assert!(pragma.has_value);
        assert_eq!(pragma.value.as_deref(), Some("7"));
    }

    #[test]
    fn skips_comments_before_pragma_values() {
        for (sql, expected) in [
            ("PRAGMA foreign_keys = /* comment */ OFF", Some("off")),
            ("PRAGMA foreign_keys = -- comment\n0", Some("0")),
            ("PRAGMA foreign_keys(/* comment */ false)", Some("false")),
            (
                "PRAGMA \"main\".\"foreign_keys\"(/* comment */ OFF)",
                Some("off"),
            ),
        ] {
            let pragma = parse_sqlite_pragma(sql).unwrap();
            assert_eq!(pragma.value.as_deref(), expected, "{sql}");
        }
    }

    #[test]
    fn preserves_quoted_and_signed_pragma_values() {
        for (sql, expected) in [
            ("PRAGMA foreign_keys = /* comment */ '-1'", Some("-1")),
            ("PRAGMA foreign_keys = -- comment\n' ON '", Some(" on ")),
            ("PRAGMA foreign_keys(/* comment */ -1)", Some("-1")),
        ] {
            let pragma = parse_sqlite_pragma(sql).unwrap();
            assert_eq!(pragma.value.as_deref(), expected, "{sql}");
        }
    }

    #[test]
    fn does_not_extract_unterminated_block_comment_as_pragma_value() {
        let pragma = parse_sqlite_pragma("PRAGMA foreign_keys = /* OFF").unwrap();

        assert_eq!(pragma.value, None);
    }
}
