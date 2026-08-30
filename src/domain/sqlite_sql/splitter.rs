use crate::sql_lex::{
    advance_single_quote, skip_block_comment, skip_double_quoted_identifier, skip_line_comment,
    skip_sqlite_quoted_identifier,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteStatementSplitError {
    UnclosedCreateTriggerBody,
    IncompleteCreateTrigger,
}

#[derive(Debug)]
pub struct SqliteStatementSplitResult<'sql> {
    statements: Vec<&'sql str>,
    error: Option<SqliteStatementSplitError>,
}

impl<'sql> SqliteStatementSplitResult<'sql> {
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

pub fn split_sqlite_statements(sql: &str) -> SqliteStatementSplitResult<'_> {
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

    // Comment-only fragments remain visible because sqlite3 uses them as statement boundaries.
    SqliteStatementSplitResult { statements, error }
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

pub(super) fn top_level_keywords(sql: &str) -> Vec<String> {
    let chars: Vec<(usize, char)> = sql.trim().char_indices().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut in_string = false;

    while i < chars.len() {
        let (byte_pos, ch) = chars[i];
        if let Some(next) = skip_line_comment(&chars, i, ch) {
            i = next;
            continue;
        }
        if let Some(next) = skip_block_comment(&chars, i, ch) {
            i = next;
            continue;
        }
        if let Some(next) = advance_single_quote(&chars, i, ch, &mut in_string) {
            i = next;
            continue;
        }
        if in_string {
            i += 1;
            continue;
        }
        if let Some(next) = skip_double_quoted_identifier(&chars, i, ch) {
            if depth == 0 {
                let end_byte = chars
                    .get(next)
                    .map_or_else(|| sql.trim().len(), |(pos, _)| *pos);
                tokens.push(sql.trim()[byte_pos..end_byte].to_string());
            }
            i = next;
            continue;
        }
        if let Some(next) = skip_sqlite_quoted_identifier(&chars, i, ch) {
            if depth == 0 {
                let end_byte = chars
                    .get(next)
                    .map_or_else(|| sql.trim().len(), |(pos, _)| *pos);
                tokens.push(sql.trim()[byte_pos..end_byte].to_string());
            }
            i = next;
            continue;
        }
        if ch == '(' {
            depth += 1;
            i += 1;
            continue;
        }
        if ch == ')' {
            depth -= 1;
            i += 1;
            continue;
        }
        if depth == 0 && (ch.is_alphanumeric() || ch == '_') {
            let start = byte_pos;
            let mut end = i + 1;
            while end < chars.len() && (chars[end].1.is_alphanumeric() || chars[end].1 == '_') {
                end += 1;
            }
            let end_byte = chars
                .get(end)
                .map_or_else(|| sql.trim().len(), |(pos, _)| *pos);
            tokens.push(sql.trim()[start..end_byte].to_ascii_uppercase());
            i = end;
            continue;
        }
        if depth == 0 && ch == ',' {
            tokens.push(",".to_string());
        }
        i += 1;
    }
    tokens
}

pub(super) fn keywords_with_depth(sql: &str) -> Vec<(String, i32)> {
    let trimmed = sql.trim();
    let chars: Vec<(usize, char)> = trimmed.char_indices().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut in_string = false;

    while i < chars.len() {
        let (byte_pos, ch) = chars[i];
        if let Some(next) = skip_line_comment(&chars, i, ch) {
            i = next;
            continue;
        }
        if let Some(next) = skip_block_comment(&chars, i, ch) {
            i = next;
            continue;
        }
        if let Some(next) = advance_single_quote(&chars, i, ch, &mut in_string) {
            i = next;
            continue;
        }
        if in_string {
            i += 1;
            continue;
        }
        if let Some(next) = skip_double_quoted_identifier(&chars, i, ch) {
            i = next;
            continue;
        }
        if let Some(next) = skip_sqlite_quoted_identifier(&chars, i, ch) {
            i = next;
            continue;
        }
        if ch == '(' {
            depth += 1;
            i += 1;
            continue;
        }
        if ch == ')' {
            depth -= 1;
            i += 1;
            continue;
        }
        if ch.is_alphanumeric() || ch == '_' {
            let start = byte_pos;
            let mut end = i + 1;
            while end < chars.len() && (chars[end].1.is_alphanumeric() || chars[end].1 == '_') {
                end += 1;
            }
            let end_byte = chars.get(end).map_or(trimmed.len(), |(pos, _)| *pos);
            tokens.push((trimmed[start..end_byte].to_ascii_uppercase(), depth));
            i = end;
            continue;
        }
        i += 1;
    }
    tokens
}

pub(super) fn first_sqlite_keyword(sql: &str) -> Option<String> {
    top_level_keywords(sql).into_iter().next()
}

pub(super) fn has_cte_body_starting_with(sql: &str, expected: &str) -> bool {
    if first_sqlite_keyword(sql).as_deref() != Some("WITH") {
        return false;
    }
    let keywords = keywords_with_depth(sql);
    keywords
        .iter()
        .enumerate()
        .any(|(index, (keyword, depth))| {
            if *depth != 0 || keyword != "AS" {
                return false;
            }
            let mut candidate = index + 1;
            while keywords
                .get(candidate)
                .is_some_and(|(_, depth)| *depth == 0)
                && keywords
                    .get(candidate)
                    .is_some_and(|(keyword, _)| matches!(keyword.as_str(), "NOT" | "MATERIALIZED"))
            {
                candidate += 1;
            }
            keywords
                .get(candidate)
                .is_some_and(|(keyword, depth)| *depth == 1 && keyword == expected)
        })
}

pub(super) fn statement_keyword(sql: &str) -> Option<String> {
    let tokens = top_level_keywords(sql);
    if tokens.first().map(String::as_str) != Some("WITH") {
        return tokens.into_iter().next();
    }

    let mut cursor = 1;
    if tokens.get(cursor).map(String::as_str) == Some("RECURSIVE") {
        cursor += 1;
    }
    loop {
        cursor += 1;
        if tokens.get(cursor).map(String::as_str) != Some("AS") {
            return Some("WITH".to_string());
        }
        cursor += 1;
        if tokens.get(cursor).map(String::as_str) == Some("NOT") {
            cursor += 1;
            if tokens.get(cursor).map(String::as_str) == Some("MATERIALIZED") {
                cursor += 1;
            }
        } else if tokens.get(cursor).map(String::as_str) == Some("MATERIALIZED") {
            cursor += 1;
        }

        match tokens.get(cursor).map(String::as_str) {
            Some(",") => cursor += 1,
            Some(keyword) => return Some(keyword.to_string()),
            None => return Some("WITH".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::empty("", Vec::<&str>::new())]
    #[case::whitespace("   ", Vec::<&str>::new())]
    #[case::empty_statements("; ; SELECT 1;;", vec!["SELECT 1"])]
    fn handles_empty_input_and_statements(#[case] sql: &str, #[case] expected: Vec<&str>) {
        let result = split_sqlite_statements(sql);

        assert_eq!(result.statements(), expected);
        assert_eq!(result.error(), None);
    }

    #[test]
    fn keeps_comment_only_input_as_a_fragment() {
        let result = split_sqlite_statements("-- comment\n/* another comment */");

        assert_eq!(
            result.statements(),
            vec!["-- comment\n/* another comment */"]
        );
    }

    #[test]
    fn keeps_comment_only_fragment_after_statement() {
        let result =
            split_sqlite_statements("INSERT INTO users(id) VALUES (1); -- trailing comment");

        assert_eq!(
            result.statements(),
            vec!["INSERT INTO users(id) VALUES (1)", "-- trailing comment"]
        );
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

    #[test]
    fn keeps_create_trigger_body_together() {
        let trigger =
            "CREATE TRIGGER t AFTER INSERT ON users BEGIN INSERT INTO logs(id) VALUES (1); END";
        let sql = format!("{trigger}; SELECT 1");
        let result = split_sqlite_statements(&sql);

        assert_eq!(result.statements(), vec![trigger, "SELECT 1"]);
        assert_eq!(result.error(), None);
    }

    #[test]
    fn reports_incomplete_create_trigger() {
        let result = split_sqlite_statements("CREATE TRIGGER t AFTER INSERT ON users");

        assert_eq!(
            result.error(),
            Some(SqliteStatementSplitError::IncompleteCreateTrigger)
        );
    }

    #[test]
    fn preserves_unicode_byte_boundaries() {
        let result = split_sqlite_statements("SELECT 'İ'; SELECT 2");

        assert_eq!(result.statements(), vec!["SELECT 'İ'", "SELECT 2"]);
    }

    #[test]
    fn recognizes_cte_statement_keyword() {
        assert_eq!(
            statement_keyword("WITH rows AS (SELECT 1) SELECT * FROM rows"),
            Some("SELECT".to_string())
        );
        assert_eq!(
            statement_keyword("WITH rows AS (SELECT 1) UPDATE users SET id = 1"),
            Some("UPDATE".to_string())
        );
    }
}
