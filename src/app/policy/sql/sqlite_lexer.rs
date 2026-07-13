use super::statement_classifier::{
    advance_single_quote, skip_block_comment, skip_double_quoted_identifier, skip_line_comment,
    skip_sqlite_quoted_identifier,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteStatementSplitError {
    UnclosedCreateTriggerBody,
    IncompleteCreateTrigger,
}

#[derive(Debug)]
pub struct SqliteStatementSplit<'sql> {
    statements: Vec<&'sql str>,
    error: Option<SqliteStatementSplitError>,
}

impl<'sql> SqliteStatementSplit<'sql> {
    pub fn statements(&self) -> &[&'sql str] {
        &self.statements
    }

    pub fn into_statements(self) -> Vec<&'sql str> {
        self.statements
    }

    pub fn error(&self) -> Option<SqliteStatementSplitError> {
        self.error
    }
}

pub fn split_sqlite_statements(sql: &str) -> SqliteStatementSplit<'_> {
    // Keep offsets from the original string so Unicode input remains sliceable.
    let chars: Vec<(usize, char)> = sql.char_indices().collect();
    let mut statements = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut in_trigger_body = false;
    let mut trigger_body_stmt_start = false;
    let mut is_trigger_stmt = is_sqlite_create_trigger_prefix(&sql[start..]);

    while i < chars.len() {
        let (byte_pos, ch) = chars[i];

        if let Some(next_i) = skip_line_comment(&chars, i, ch) {
            i = next_i;
            continue;
        }
        if let Some(next_i) = skip_block_comment(&chars, i, ch) {
            i = next_i;
            continue;
        }
        if let Some(next_i) = advance_single_quote(&chars, i, ch, &mut in_string) {
            i = next_i;
            continue;
        }
        if in_string {
            i += 1;
            continue;
        }
        if let Some(next_i) = skip_double_quoted_identifier(&chars, i, ch) {
            i = next_i;
            continue;
        }
        if let Some(next_i) = skip_sqlite_quoted_identifier(&chars, i, ch) {
            i = next_i;
            continue;
        }

        if is_trigger_stmt && let Some((keyword, keyword_end)) = keyword_starting_at(sql, &chars, i)
        {
            if keyword == "BEGIN" {
                if in_trigger_body {
                    trigger_body_stmt_start = false;
                } else {
                    in_trigger_body = true;
                    trigger_body_stmt_start = true;
                }
            } else if is_trigger_body_end(
                &keyword,
                in_trigger_body,
                trigger_body_stmt_start,
                sql,
                byte_pos,
            ) {
                in_trigger_body = false;
                trigger_body_stmt_start = false;
            } else if in_trigger_body {
                trigger_body_stmt_start = false;
            }
            i = keyword_end;
            continue;
        }

        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
        }

        if depth == 0 && ch == ';' {
            if in_trigger_body {
                trigger_body_stmt_start = true;
            } else {
                push_statement(sql, start, byte_pos, &mut statements);
                start = byte_pos + 1;
                in_trigger_body = false;
                trigger_body_stmt_start = false;
                is_trigger_stmt = is_sqlite_create_trigger_prefix(&sql[start..]);
            }
        }

        i += 1;
    }

    let mut error = None;
    if start < sql.len() {
        let fragment = sql[start..].trim();
        if !fragment.is_empty() {
            statements.push(fragment);
            if in_trigger_body {
                error = Some(SqliteStatementSplitError::UnclosedCreateTriggerBody);
            } else if is_sqlite_create_trigger_prefix(fragment)
                && !contains_keyword(fragment, "BEGIN")
            {
                error = Some(SqliteStatementSplitError::IncompleteCreateTrigger);
            }
        }
    }

    statements.retain(|statement| !is_comment_only(statement));

    SqliteStatementSplit { statements, error }
}

fn push_statement<'sql>(sql: &'sql str, start: usize, end: usize, statements: &mut Vec<&'sql str>) {
    let fragment = sql[start..end].trim();
    if !fragment.is_empty() {
        statements.push(fragment);
    }
}

fn is_sqlite_create_trigger_prefix(sql: &str) -> bool {
    let trimmed = sql.trim_start();
    let chars: Vec<(usize, char)> = trimmed.char_indices().collect();

    let Some((first, second_start)) = next_keyword_from(trimmed, &chars, 0) else {
        return false;
    };
    if first != "CREATE" {
        return false;
    }
    let Some((second, third_start)) = next_keyword_from(trimmed, &chars, second_start) else {
        return false;
    };
    match second.as_str() {
        "TRIGGER" => true,
        "TEMP" | "TEMPORARY" => next_keyword_from(trimmed, &chars, third_start)
            .is_some_and(|(third, _)| third == "TRIGGER"),
        _ => false,
    }
}

fn next_keyword_from(sql: &str, chars: &[(usize, char)], mut i: usize) -> Option<(String, usize)> {
    let mut in_string = false;
    while i < chars.len() {
        let (_, ch) = chars[i];
        if let Some(next_i) = skip_line_comment(chars, i, ch) {
            i = next_i;
            continue;
        }
        if let Some(next_i) = skip_block_comment(chars, i, ch) {
            i = next_i;
            continue;
        }
        if let Some(next_i) = advance_single_quote(chars, i, ch, &mut in_string) {
            i = next_i;
            continue;
        }
        if in_string {
            i += 1;
            continue;
        }
        if let Some(next_i) = skip_double_quoted_identifier(chars, i, ch) {
            i = next_i;
            continue;
        }
        if let Some(next_i) = skip_sqlite_quoted_identifier(chars, i, ch) {
            i = next_i;
            continue;
        }
        if let Some(keyword) = keyword_starting_at(sql, chars, i) {
            return Some(keyword);
        }
        i += 1;
    }
    None
}

fn keyword_starting_at(sql: &str, chars: &[(usize, char)], i: usize) -> Option<(String, usize)> {
    let (byte_pos, ch) = chars[i];
    if !ch.is_ascii_alphabetic() {
        return None;
    }
    let start = byte_pos;
    let mut end = i;
    while end < chars.len() && (chars[end].1.is_ascii_alphanumeric() || chars[end].1 == '_') {
        end += 1;
    }
    let end_byte = chars.get(end).map_or(sql.len(), |(byte_pos, _)| *byte_pos);
    Some((sql[start..end_byte].to_ascii_uppercase(), end))
}

fn contains_keyword(sql: &str, expected: &str) -> bool {
    let chars: Vec<(usize, char)> = sql.char_indices().collect();
    let mut offset = 0;
    while let Some((keyword, end)) = next_keyword_from(sql, &chars, offset) {
        if keyword == expected {
            return true;
        }
        offset = end;
    }
    false
}

fn is_trigger_body_end(
    keyword: &str,
    in_trigger_body: bool,
    trigger_body_stmt_start: bool,
    sql: &str,
    keyword_start: usize,
) -> bool {
    in_trigger_body
        && trigger_body_stmt_start
        && keyword == "END"
        && !is_dotted_identifier_suffix(sql, keyword_start)
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

fn is_comment_only(sql: &str) -> bool {
    let chars: Vec<(usize, char)> = sql.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (_, ch) = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if let Some(next_i) = skip_line_comment(&chars, i, ch) {
            i = next_i;
            continue;
        }
        if let Some(next_i) = skip_block_comment(&chars, i, ch) {
            i = next_i;
            continue;
        }
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::empty("", Vec::<&str>::new())]
    #[case::whitespace("   ", Vec::<&str>::new())]
    #[case::empty_statements("; ; SELECT 1;;", vec!["SELECT 1"])]
    #[case::comment_only("-- comment\n/* another comment */", Vec::<&str>::new())]
    fn handles_empty_input_and_statements(#[case] sql: &str, #[case] expected: Vec<&str>) {
        let result = split_sqlite_statements(sql);

        assert_eq!(result.statements(), expected);
        assert_eq!(result.error(), None);
    }

    #[rstest]
    #[case::single("SELECT 1", vec!["SELECT 1"])]
    #[case::multiple("SELECT 1; SELECT 2", vec!["SELECT 1", "SELECT 2"])]
    #[case::trailing_semicolon("SELECT 1;", vec!["SELECT 1"])]
    fn splits_sqlite_statements(#[case] sql: &str, #[case] expected: Vec<&str>) {
        let result = split_sqlite_statements(sql);

        assert_eq!(result.statements(), expected);
        assert_eq!(result.error(), None);
    }

    #[rstest]
    #[case::single_quote("SELECT ';'; SELECT 1", vec!["SELECT ';'", "SELECT 1"])]
    #[case::double_quote("SELECT \";\"; SELECT 1", vec!["SELECT \";\"", "SELECT 1"])]
    #[case::backtick("SELECT `;`; SELECT 1", vec!["SELECT `;`", "SELECT 1"])]
    #[case::bracket("SELECT [;]; SELECT 1", vec!["SELECT [;]", "SELECT 1"])]
    fn ignores_semicolons_in_sqlite_quotes(#[case] sql: &str, #[case] expected: Vec<&str>) {
        let result = split_sqlite_statements(sql);

        assert_eq!(result.statements(), expected);
        assert_eq!(result.error(), None);
    }

    #[rstest]
    #[case::line_comment("SELECT 1 -- ; ignored\n; SELECT 2", vec!["SELECT 1 -- ; ignored", "SELECT 2"])]
    #[case::block_comment("SELECT /* ; ignored */ 1; SELECT 2", vec!["SELECT /* ; ignored */ 1", "SELECT 2"])]
    fn ignores_semicolons_in_comments(#[case] sql: &str, #[case] expected: Vec<&str>) {
        let result = split_sqlite_statements(sql);

        assert_eq!(result.statements(), expected);
        assert_eq!(result.error(), None);
    }

    #[test]
    fn keeps_create_trigger_body_together() {
        let trigger = "\
CREATE TRIGGER agent_messages_fts_ai AFTER INSERT ON agent_messages BEGIN
    INSERT INTO agent_messages_fts(rowid, role, content)
    VALUES (new.id, new.role, new.content);
END";
        let sql = format!("{trigger}; SELECT 1");
        let result = split_sqlite_statements(&sql);

        assert_eq!(result.statements(), vec![trigger, "SELECT 1"]);
        assert_eq!(result.error(), None);
    }

    #[test]
    fn keeps_trigger_end_column_references_inside_body() {
        let trigger = "\
CREATE TRIGGER sync_end AFTER UPDATE ON events BEGIN
    UPDATE counters SET end_value = new.end WHERE id = new.id;
    INSERT INTO audit(event_id, end_value) VALUES (new.id, old.end);
END";
        let sql = format!("{trigger}; SELECT 1");
        let result = split_sqlite_statements(&sql);

        assert_eq!(result.statements(), vec![trigger, "SELECT 1"]);
        assert_eq!(result.error(), None);
    }

    #[test]
    fn reports_unfinished_create_trigger_body_without_changing_statements() {
        let trigger =
            "CREATE TRIGGER t AFTER INSERT ON users BEGIN INSERT INTO logs(id) VALUES (1);";
        let result = split_sqlite_statements(trigger);

        assert_eq!(result.statements(), vec![trigger]);
        assert_eq!(
            result.error(),
            Some(SqliteStatementSplitError::UnclosedCreateTriggerBody)
        );
    }

    #[test]
    fn reports_incomplete_create_trigger() {
        let result = split_sqlite_statements("CREATE TRIGGER t AFTER INSERT ON users");

        assert_eq!(
            result.statements(),
            vec!["CREATE TRIGGER t AFTER INSERT ON users"]
        );
        assert_eq!(
            result.error(),
            Some(SqliteStatementSplitError::IncompleteCreateTrigger)
        );
    }

    #[test]
    fn keeps_unclosed_string_as_a_statement() {
        let result = split_sqlite_statements("SELECT 'unclosed");

        assert_eq!(result.statements(), vec!["SELECT 'unclosed"]);
        assert_eq!(result.error(), None);
    }

    #[test]
    fn preserves_unicode_byte_boundaries() {
        let result = split_sqlite_statements("SELECT 'İ'; SELECT 2");

        assert_eq!(result.statements(), vec!["SELECT 'İ'", "SELECT 2"]);
    }

    #[test]
    fn recognizes_temporary_trigger() {
        let result = split_sqlite_statements(
            "CREATE TEMPORARY TRIGGER t AFTER INSERT ON users BEGIN SELECT 1; END; SELECT 2",
        );

        assert_eq!(result.statements().len(), 2);
        assert_eq!(result.error(), None);
    }
}
