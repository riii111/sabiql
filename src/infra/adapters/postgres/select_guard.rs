/// PostgreSQL allows DML inside CTEs (e.g., `WITH ... UPDATE`), so prefix-only
/// checks are unsafe. We must inspect top-level SQL tokens.
pub(in crate::infra::adapters::postgres) fn is_select_query(query: &str) -> bool {
    let lower = query.trim().to_lowercase();
    let chars: Vec<(usize, char)> = lower.char_indices().collect();
    check_select_safety(&lower, &chars)
}

fn check_select_safety(lower: &str, chars: &[(usize, char)]) -> bool {
    let mut state = ParseState::default();

    while state.i < chars.len() {
        let (byte_pos, ch) = chars[state.i];

        if let Some(next_i) = skip_line_comment(chars, state.i, ch) {
            state.i = next_i;
            continue;
        }
        if let Some(next_i) = skip_block_comment(chars, state.i, ch) {
            state.i = next_i;
            continue;
        }
        if let Some(next_i) = advance_single_quote(chars, state.i, ch, &mut state.in_string) {
            state.i = next_i;
            continue;
        }
        if state.in_string {
            state.i += 1;
            continue;
        }
        if let Some(next_i) = skip_double_quoted_identifier(chars, state.i, ch) {
            state.i = next_i;
            continue;
        }
        if let Some(next_i) = skip_dollar_quoted_string(lower, chars, state.i, byte_pos, ch) {
            state.i = next_i;
            continue;
        }

        update_parentheses_depth(ch, &mut state.depth);

        if state.depth == 0 && ch == ';' {
            if has_non_whitespace_after_semicolon(lower, byte_pos) {
                return false;
            }
            break;
        }

        if is_word_start(chars, state.i) {
            let rest = &lower[byte_pos..];
            if state.depth == 0 && is_keyword(rest, "select") {
                state.found_select = true;
            }
            // SELECT INTO creates a table, so SQL modal must reject it.
            if state.depth == 0 && state.found_select && is_keyword(rest, "into") {
                return false;
            }
            if is_rejected_keyword(rest) {
                return false;
            }
        }

        state.i += 1;
    }

    state.found_select
}

#[derive(Default)]
struct ParseState {
    i: usize,
    depth: i32,
    in_string: bool,
    found_select: bool,
}

fn skip_line_comment(chars: &[(usize, char)], i: usize, ch: char) -> Option<usize> {
    if ch != '-' || !next_char_is(chars, i, '-') {
        return None;
    }

    let mut cursor = i;
    while cursor < chars.len() && chars[cursor].1 != '\n' {
        cursor += 1;
    }
    Some(cursor)
}

fn skip_block_comment(chars: &[(usize, char)], i: usize, ch: char) -> Option<usize> {
    if ch != '/' || !next_char_is(chars, i, '*') {
        return None;
    }

    let mut cursor = i + 2;
    while cursor + 1 < chars.len() && !(chars[cursor].1 == '*' && chars[cursor + 1].1 == '/') {
        cursor += 1;
    }
    Some(cursor + 2)
}

fn advance_single_quote(
    chars: &[(usize, char)],
    i: usize,
    ch: char,
    in_string: &mut bool,
) -> Option<usize> {
    if ch != '\'' {
        return None;
    }

    if *in_string {
        if next_char_is(chars, i, '\'') {
            return Some(i + 2);
        }
        *in_string = false;
    } else {
        *in_string = true;
    }
    Some(i + 1)
}

fn skip_double_quoted_identifier(chars: &[(usize, char)], i: usize, ch: char) -> Option<usize> {
    if ch != '"' {
        return None;
    }

    let mut cursor = i + 1;
    while cursor < chars.len() {
        if chars[cursor].1 == '"' {
            if next_char_is(chars, cursor, '"') {
                cursor += 2;
            } else {
                cursor += 1;
                break;
            }
        } else {
            cursor += 1;
        }
    }
    Some(cursor)
}

fn skip_dollar_quoted_string(
    lower: &str,
    chars: &[(usize, char)],
    i: usize,
    byte_pos: usize,
    ch: char,
) -> Option<usize> {
    if ch != '$' {
        return None;
    }

    let mut cursor = i + 1;
    while cursor < chars.len() && (chars[cursor].1.is_alphanumeric() || chars[cursor].1 == '_') {
        cursor += 1;
    }
    if cursor >= chars.len() || chars[cursor].1 != '$' {
        return None;
    }

    let tag = &lower[byte_pos..=chars[cursor].0];
    cursor += 1;

    while cursor + tag.len() <= chars.len() {
        let candidate_start = chars[cursor].0;
        if chars[cursor].1 == '$' {
            let candidate_end = candidate_start + tag.len();
            if candidate_end <= lower.len() && &lower[candidate_start..candidate_end] == tag {
                let mut next = cursor;
                while next < chars.len() && chars[next].0 < candidate_end {
                    next += 1;
                }
                return Some(next);
            }
        }
        cursor += 1;
    }

    // Treat an unclosed dollar-quote as "consume until end". This keeps keyword
    // scanning out of unterminated string-like input.
    Some(cursor)
}

fn update_parentheses_depth(ch: char, depth: &mut i32) {
    if ch == '(' {
        *depth += 1;
    } else if ch == ')' {
        *depth -= 1;
    }
}

fn has_non_whitespace_after_semicolon(lower: &str, byte_pos: usize) -> bool {
    lower
        .get(byte_pos + 1..)
        .map(|tail| !tail.trim().is_empty())
        .unwrap_or(false)
}

fn is_rejected_keyword(rest: &str) -> bool {
    // Writable CTEs place DML inside subqueries, so rejection cannot be
    // limited to top-level keywords.
    is_keyword(rest, "insert")
        || is_keyword(rest, "update")
        || is_keyword(rest, "delete")
        || is_keyword(rest, "create")
}

fn next_char_is(chars: &[(usize, char)], i: usize, expected: char) -> bool {
    i + 1 < chars.len() && chars[i + 1].1 == expected
}

fn is_word_start(chars: &[(usize, char)], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = chars[i - 1].1;
    !prev.is_alphanumeric() && prev != '_'
}

fn is_keyword(s: &str, keyword: &str) -> bool {
    if !s.starts_with(keyword) {
        return false;
    }
    s[keyword.len()..]
        .chars()
        .next()
        .map(|c| !c.is_alphanumeric() && c != '_')
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod select_validation {
        use super::is_select_query;
        use rstest::rstest;

        fn assert_query(query: &str, expected: bool) {
            assert_eq!(is_select_query(query), expected);
        }

        #[rstest]
        #[case::plain_select("SELECT * FROM users", true)]
        #[case::lowercase_select("select id from users", true)]
        #[case::trimmed_select("  SELECT id FROM users  ", true)]
        #[case::cte_select("WITH cte AS (SELECT 1) SELECT * FROM cte", true)]
        #[case::recursive_cte_select("with recursive tree AS (SELECT 1) SELECT * FROM tree", true)]
        #[case::multiple_ctes_select(
            "WITH a AS (SELECT 1), b AS (SELECT 2) SELECT * FROM a, b",
            true
        )]
        #[case::select_with_parenthesized_expr("WITH cte AS (SELECT 1) SELECT (1+2)", true)]
        #[case::select_with_subquery_expr("WITH cte AS (SELECT 1) SELECT (SELECT 1)", true)]
        fn basic_select_accepted(#[case] query: &str, #[case] expected: bool) {
            assert_query(query, expected);
        }

        #[rstest]
        #[case::cte_update("WITH cte AS (SELECT 1) UPDATE users SET name = 'x'", false)]
        #[case::cte_delete("WITH cte AS (SELECT 1) DELETE FROM users", false)]
        #[case::cte_insert("WITH cte AS (SELECT 1) INSERT INTO users VALUES (1)", false)]
        #[case::cte_update_lowercase("with cte as (select 1) update users set name = 'x'", false)]
        #[case::plain_insert("INSERT INTO users VALUES (1)", false)]
        #[case::plain_insert_with_whitespace("  insert into users (id) values (1)", false)]
        #[case::plain_update("UPDATE users SET name = 'new'", false)]
        #[case::plain_update_with_whitespace("  update users set active = true", false)]
        #[case::plain_delete("DELETE FROM users WHERE id = 1", false)]
        #[case::plain_delete_with_whitespace("  delete from users", false)]
        #[case::create_table("CREATE TABLE foo (id INT)", false)]
        #[case::drop_table("DROP TABLE users", false)]
        #[case::alter_table("ALTER TABLE users ADD COLUMN foo INT", false)]
        #[case::truncate_table("TRUNCATE users", false)]
        #[case::select_into("SELECT * INTO new_table FROM old_table", false)]
        #[case::select_into_columns("SELECT id, name INTO backup FROM users", false)]
        #[case::create_table_as_select("CREATE TABLE t AS SELECT * FROM users", false)]
        #[case::create_table_as_select_columns(
            "CREATE TABLE backup AS SELECT id FROM users",
            false
        )]
        #[case::writable_cte_update_returning(
            "WITH x AS (UPDATE users SET name='a' RETURNING *) SELECT * FROM x",
            false
        )]
        #[case::writable_cte_delete_returning(
            "WITH x AS (DELETE FROM users RETURNING *) SELECT * FROM x",
            false
        )]
        fn mutation_and_schema_change_rejected(#[case] query: &str, #[case] expected: bool) {
            assert_query(query, expected);
        }

        #[rstest]
        #[case::multiple_statement_delete("SELECT 1; DELETE FROM users", false)]
        #[case::multiple_statement_update("SELECT * FROM t; UPDATE t SET x = 1", false)]
        #[case::multiple_statement_select("SELECT 1; SELECT 2", false)]
        #[case::semicolon_inside_string("SELECT * FROM t WHERE x = ';'", true)]
        #[case::trailing_semicolon("SELECT * FROM users;", true)]
        #[case::trailing_semicolon_select_literal("SELECT 1;", true)]
        #[case::trailing_semicolon_predicate("SELECT * FROM t WHERE x = 1;", true)]
        #[case::trailing_semicolon_cte("WITH cte AS (SELECT 1) SELECT * FROM cte;", true)]
        #[case::trailing_semicolon_whitespace_only("SELECT 1;   ", true)]
        #[case::semicolon_followed_by_comment("SELECT 1; -- done", false)]
        fn statement_boundary_rules(#[case] query: &str, #[case] expected: bool) {
            assert_query(query, expected);
        }

        #[rstest]
        #[case::string_with_parenthesis("WITH cte AS (SELECT '(' FROM t) SELECT * FROM cte", true)]
        #[case::string_parenthesized_text("SELECT * FROM t WHERE name = '(test)'", true)]
        #[case::string_with_escaped_quote("SELECT * FROM t WHERE name = 'it''s'", true)]
        #[case::cte_string_with_escaped_quote(
            "WITH cte AS (SELECT 'a''b') SELECT * FROM cte",
            true
        )]
        #[case::string_contains_delete("SELECT * FROM t WHERE action = 'delete'", true)]
        #[case::string_contains_insert_into("SELECT * FROM t WHERE cmd = 'INSERT INTO'", true)]
        #[case::identifier_contains_delete("SELECT mydelete FROM t", true)]
        #[case::identifier_contains_delete_prefix("SELECT delete_flag FROM t", true)]
        #[case::cte_name_contains_delete(
            "WITH mydelete AS (SELECT 1) SELECT * FROM mydelete",
            true
        )]
        #[case::table_name_contains_delete("SELECT * FROM users_to_delete", true)]
        #[case::double_quoted_keyword("SELECT \"update\" FROM t", true)]
        #[case::double_quoted_alias_keyword(
            "WITH x AS (SELECT 1 AS \"delete\") SELECT * FROM x",
            true
        )]
        #[case::double_quoted_escaped_identifier("SELECT \"up\"\"date\" FROM t", true)]
        #[case::dollar_quoted_keyword("SELECT $$update$$ AS label", true)]
        #[case::tagged_dollar_quoted_keyword("SELECT $tag$delete from here$tag$ AS s", true)]
        #[case::dollar_quoted_with_semicolon("SELECT $$semi;colon$$ AS label", true)]
        #[case::into_in_subquery("SELECT * FROM (SELECT 1) AS sub", true)]
        #[case::into_in_string("SELECT * FROM t WHERE x = 'INTO'", true)]
        fn keywords_inside_literals_or_identifiers_allowed(
            #[case] query: &str,
            #[case] expected: bool,
        ) {
            assert_query(query, expected);
        }

        #[rstest]
        #[case::line_comment_with_keyword("-- delete old records\nSELECT * FROM t", true)]
        #[case::block_comment_with_keyword("/* update cache */ SELECT * FROM t", true)]
        #[case::trailing_comment_with_keyword("SELECT * FROM t -- insert comment", true)]
        #[case::inline_block_comment_with_keyword("SELECT /* delete */ * FROM t", true)]
        #[case::empty_input("", false)]
        #[case::whitespace_only_input("   ", false)]
        #[case::non_ascii_identifier("SELECT * FROM \"ユーザー\"", true)]
        #[case::non_ascii_literal("SELECT name FROM users WHERE name = '日本語'", true)]
        #[case::non_ascii_literal_in_cte("WITH cte AS (SELECT '中文') SELECT * FROM cte", true)]
        #[case::unterminated_dollar_quote("SELECT $$unclosed", true)]
        #[case::nested_block_comment_with_safe_leak(
            "SELECT /* outer /* inner */ still comment */ 1",
            true
        )]
        #[case::nested_block_comment_with_rejected_leak("SELECT /* /* */ delete */ 1", false)]
        fn edge_cases_and_comment_behavior(#[case] query: &str, #[case] expected: bool) {
            assert_query(query, expected);
        }
    }
}
