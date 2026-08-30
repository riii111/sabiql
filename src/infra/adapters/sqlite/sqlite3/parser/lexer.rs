use crate::app::ports::outbound::DbOperationError;
use crate::domain::sqlite_sql::{
    SqliteStatementSplitError, is_sqlite_rerunnable_export_statement, split_sqlite_statements,
};

fn is_ident_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub(in crate::adapters::sqlite) fn skip_quoted(bytes: &[u8], mut i: usize, quote: u8) -> usize {
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

pub(in crate::adapters::sqlite) fn skip_bracket_quoted(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b']' {
            return i + 1;
        }
        i += 1;
    }
    i
}

const SQLITE_FSDIR_SAFE_MODE_ERROR: &str = "SQLite fsdir access is not supported in safe mode";

fn quoted_identifier_matches(bytes: &[u8], start: usize, quote: u8, expected: &str) -> bool {
    let mut i = start + 1;
    let content_start = i;
    while i < bytes.len() {
        if bytes[i] == quote {
            if i + 1 < bytes.len() && bytes[i + 1] == quote {
                i += 2;
            } else {
                return bytes[content_start..i].eq_ignore_ascii_case(expected.as_bytes());
            }
        } else {
            i += 1;
        }
    }
    false
}

fn bracket_identifier_matches(bytes: &[u8], start: usize, expected: &str) -> bool {
    let end = skip_bracket_quoted(bytes, start);
    end > start + 1
        && end <= bytes.len()
        && bytes[start + 1..end - 1].eq_ignore_ascii_case(expected.as_bytes())
}

fn skip_sqlite_trivia(bytes: &[u8], mut i: usize) -> usize {
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        if i + 1 < bytes.len() && bytes[i] == b'-' && bytes[i + 1] == b'-' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            if i + 1 < bytes.len() {
                i += 2;
            }
            continue;
        }

        return i;
    }
}

fn is_followed_by_open_parenthesis(bytes: &[u8], end: usize) -> bool {
    let next = skip_sqlite_trivia(bytes, end);
    next < bytes.len() && bytes[next] == b'('
}

fn contains_sqlite_fsdir_access(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut i = 0;
    let mut previous_token_was_using = false;
    let mut create_statement = false;
    let mut create_table_name_context = false;

    while i < bytes.len() {
        let next = skip_sqlite_trivia(bytes, i);
        if next == bytes.len() {
            break;
        }
        i = next;

        let start = i;
        let mut token_was_using = false;
        match bytes[i] {
            b'\'' | b'"' | b'`' => {
                let quote = bytes[i];
                let matches = quoted_identifier_matches(bytes, i, quote, "fsdir");
                i = skip_quoted(bytes, i, quote);
                if matches
                    && (previous_token_was_using
                        || (!create_table_name_context
                            && is_followed_by_open_parenthesis(bytes, i)))
                {
                    return true;
                }
            }
            b'[' => {
                let matches = bracket_identifier_matches(bytes, i, "fsdir");
                i = skip_bracket_quoted(bytes, i);
                if matches
                    && (previous_token_was_using
                        || (!create_table_name_context
                            && is_followed_by_open_parenthesis(bytes, i)))
                {
                    return true;
                }
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                while i < bytes.len() && is_ident_char(bytes[i]) {
                    i += 1;
                }
                let matches = bytes[start..i].eq_ignore_ascii_case(b"fsdir");
                if bytes[start..i].eq_ignore_ascii_case(b"create") {
                    create_statement = true;
                    create_table_name_context = false;
                } else if create_statement && bytes[start..i].eq_ignore_ascii_case(b"table") {
                    create_table_name_context = true;
                } else if bytes[start..i].eq_ignore_ascii_case(b"using")
                    || bytes[start..i].eq_ignore_ascii_case(b"as")
                {
                    create_table_name_context = false;
                }
                if matches
                    && (previous_token_was_using
                        || (!create_table_name_context
                            && is_followed_by_open_parenthesis(bytes, i)))
                {
                    return true;
                }
                token_was_using = bytes[start..i].eq_ignore_ascii_case(b"using");
            }
            _ => {
                if bytes[i] == b';' {
                    create_statement = false;
                    create_table_name_context = false;
                } else if bytes[i] == b'(' {
                    create_table_name_context = false;
                }
                i += 1;
            }
        }
        previous_token_was_using = token_was_using;
    }

    false
}

pub(in crate::adapters::sqlite) fn reject_sqlite_fsdir(sql: &str) -> Result<(), DbOperationError> {
    if contains_sqlite_fsdir_access(sql) {
        return Err(DbOperationError::UnsupportedOperation(
            SQLITE_FSDIR_SAFE_MODE_ERROR.to_string(),
        ));
    }
    Ok(())
}

/// Returns the next SQL keyword and the byte offset immediately after it.
pub(in crate::adapters::sqlite) fn next_keyword_from(
    sql: &str,
    mut i: usize,
) -> Option<(&str, usize)> {
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

pub(super) fn contains_keyword(sql: &str, expected: &str) -> bool {
    let mut offset = 0;
    while let Some((keyword, end)) = next_keyword_from(sql, offset) {
        if keyword.eq_ignore_ascii_case(expected) {
            return true;
        }
        offset = end;
    }
    false
}

fn is_create_keyword_prefix(sql: &str, keyword: &str) -> bool {
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
        return third.eq_ignore_ascii_case(keyword);
    }
    second.eq_ignore_ascii_case(keyword)
}

pub(in crate::adapters::sqlite) fn is_create_virtual_table_prefix(sql: &str) -> bool {
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

pub(in crate::adapters::sqlite) fn is_create_view_prefix(sql: &str) -> bool {
    is_create_keyword_prefix(sql, "VIEW")
}

pub(in crate::adapters::sqlite) fn virtual_table_module_name(sql: &str) -> Option<String> {
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

pub(in crate::adapters::sqlite::sqlite3) fn try_split_sqlite_statements(
    sql: &str,
) -> Result<Vec<&str>, DbOperationError> {
    reject_sqlite_meta_commands(sql)?;
    reject_sqlite_fsdir(sql)?;
    let split = split_sqlite_statements(sql);
    if let Some(error) = split.error() {
        let error = match error {
            SqliteStatementSplitError::UnclosedCreateTriggerBody => "Unclosed CREATE TRIGGER body",
            SqliteStatementSplitError::IncompleteCreateTrigger => {
                "Incomplete CREATE TRIGGER statement"
            }
        };
        return Err(DbOperationError::QueryFailed(error.to_string()));
    }
    Ok(split.into_statements())
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

pub(in crate::adapters::sqlite) fn is_sqlite_rerunnable_export_query(
    query: &str,
) -> Result<bool, DbOperationError> {
    let statements = try_split_sqlite_statements(query)?;
    Ok(statements.len() == 1
        && statements
            .iter()
            .all(|statement| is_sqlite_rerunnable_export_statement(statement)))
}

pub(in crate::adapters::sqlite) fn sqlite_export_not_rerunnable_error() -> DbOperationError {
    DbOperationError::UnsupportedOperation(
        "Cannot re-execute this query for CSV export because it contains write or DDL statements"
            .to_string(),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod statement_splitting {
        use super::*;

        #[rstest]
        #[case::unquoted("SELECT * FROM fsdir('/tmp')")]
        #[case::double_quoted("SELECT * FROM \"fsdir\"('/tmp')")]
        #[case::backtick_quoted("SELECT * FROM `fsdir`('/tmp')")]
        #[case::bracket_quoted("SELECT * FROM [fsdir]('/tmp')")]
        #[case::single_quoted("SELECT * FROM 'fsdir'('/tmp')")]
        #[case::comments_between_tokens("SELECT * FROM /* ignored */ fsdir /* ignored */ ('/tmp')")]
        #[case::virtual_table_module("CREATE VIRTUAL TABLE files USING fsdir")]
        #[case::single_quoted_virtual_table_module("CREATE VIRTUAL TABLE files USING 'fsdir'")]
        #[case::double_quoted_virtual_table_module("CREATE VIRTUAL TABLE files USING \"fsdir\"")]
        #[case::backtick_virtual_table_module("CREATE VIRTUAL TABLE files USING `fsdir`")]
        #[case::bracket_virtual_table_module("CREATE VIRTUAL TABLE files USING [fsdir]")]
        fn rejects_fsdir_access(#[case] sql: &str) {
            let error = try_split_sqlite_statements(sql).unwrap_err();

            assert!(matches!(
                error,
                DbOperationError::UnsupportedOperation(details)
                    if details == SQLITE_FSDIR_SAFE_MODE_ERROR
            ));
        }

        #[rstest]
        #[case::single_quoted_literal("SELECT 'fsdir'")]
        #[case::line_comment("-- fsdir\nSELECT 1")]
        #[case::block_comment("/* fsdir */ SELECT 1")]
        #[case::identifier_prefix("SELECT fsdirname FROM users")]
        #[case::identifier_suffix("SELECT myfsdir FROM users")]
        #[case::column("SELECT fsdir FROM users")]
        #[case::alias("SELECT 1 AS fsdir")]
        #[case::table_without_arguments("SELECT * FROM fsdir")]
        #[case::table_definition("CREATE TABLE fsdir(name TEXT)")]
        #[case::quoted_table_definition("CREATE TABLE IF NOT EXISTS 'fsdir'(name TEXT)")]
        #[case::other_virtual_table("SELECT * FROM json_each('{}')")]
        fn ignores_non_fsdir_occurrences(#[case] sql: &str) {
            assert!(try_split_sqlite_statements(sql).is_ok());
        }

        #[test]
        fn rejects_fsdir_in_any_statement_of_batch() {
            let error =
                try_split_sqlite_statements("SELECT 1; SELECT * FROM fsdir('/tmp')").unwrap_err();

            assert!(matches!(
                error,
                DbOperationError::UnsupportedOperation(details)
                    if details == SQLITE_FSDIR_SAFE_MODE_ERROR
            ));
        }

        #[test]
        fn ignores_semicolons_in_literals_and_comments() {
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
        fn rejects_dot_commands() {
            let error = try_split_sqlite_statements("SELECT 1;\n.shell echo unsafe").unwrap_err();

            assert!(matches!(error, DbOperationError::UnsupportedOperation(_)));
        }

        #[test]
        fn allows_dot_at_line_start_inside_literal() {
            let statements =
                try_split_sqlite_statements("SELECT '.shell echo safe\n.read file';").unwrap();

            assert_eq!(statements, vec!["SELECT '.shell echo safe\n.read file'"]);
        }

        #[test]
        fn keeps_create_trigger_body_together() {
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
        fn keeps_create_trigger_with_dotted_end_reference() {
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
        fn keeps_create_trigger_with_case_end_expression() {
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
        fn rejects_unclosed_create_trigger_body() {
            let error = try_split_sqlite_statements(
                "CREATE TRIGGER t AFTER INSERT ON users BEGIN INSERT INTO logs(id) VALUES (1);",
            )
            .unwrap_err();

            assert!(matches!(error, DbOperationError::QueryFailed(_)));
        }

        #[test]
        fn rejects_incomplete_create_trigger_without_begin() {
            let error =
                try_split_sqlite_statements("CREATE TRIGGER t AFTER INSERT ON users").unwrap_err();

            assert!(matches!(error, DbOperationError::QueryFailed(_)));
        }
    }

    mod export_guard {
        use super::*;

        #[test]
        fn rejects_non_rerunnable_sql() {
            for sql in [
                "SELECT 1; SELECT 2",
                "WITH payload(id) AS (VALUES (1)) INSERT INTO users(id) SELECT id FROM payload",
                "PRAGMA foreign_keys=OFF",
                "PRAGMA journal_mode=WAL",
                "PRAGMA wal_checkpoint(TRUNCATE)",
            ] {
                assert!(!is_sqlite_rerunnable_export_query(sql).unwrap(), "{sql}");
            }
        }

        #[test]
        fn allows_read_only_sql() {
            for sql in ["SELECT 1", "PRAGMA table_info(users)"] {
                assert!(is_sqlite_rerunnable_export_query(sql).unwrap(), "{sql}");
            }
        }
    }

    mod virtual_table_parsing {
        use super::*;

        #[test]
        fn prefix_requires_keyword_sequence() {
            assert!(is_create_virtual_table_prefix(
                "CREATE VIRTUAL TABLE notes_fts USING fts5(body);"
            ));
            assert!(!is_create_virtual_table_prefix(
                "CREATE TABLE docs(body TEXT DEFAULT 'create virtual table');"
            ));
        }

        #[test]
        fn module_name_skips_quoted_table_name() {
            assert_eq!(
                virtual_table_module_name(r#"CREATE VIRTUAL TABLE "using" USING fts5(body);"#),
                Some("fts5".to_string())
            );
        }

        #[test]
        fn module_name_reads_double_quoted_module() {
            assert_eq!(
                virtual_table_module_name(r#"CREATE VIRTUAL TABLE notes USING "fts5"(body);"#),
                Some("fts5".to_string())
            );
        }

        #[test]
        fn module_name_rejects_unclosed_bracket_module() {
            assert_eq!(
                virtual_table_module_name("CREATE VIRTUAL TABLE notes USING [fts5(body);"),
                None
            );
        }
    }
}
