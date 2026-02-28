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

        #[rstest]
        // Basic SELECT
        #[case("SELECT * FROM users", true)]
        #[case("select id from users", true)]
        #[case("  SELECT id FROM users  ", true)]
        // CTE with SELECT (allowed)
        #[case("WITH cte AS (SELECT 1) SELECT * FROM cte", true)]
        #[case("with recursive tree AS (SELECT 1) SELECT * FROM tree", true)]
        #[case("WITH a AS (SELECT 1), b AS (SELECT 2) SELECT * FROM a, b", true)]
        // CTE with DML (rejected)
        #[case("WITH cte AS (SELECT 1) UPDATE users SET name = 'x'", false)]
        #[case("WITH cte AS (SELECT 1) DELETE FROM users", false)]
        #[case("WITH cte AS (SELECT 1) INSERT INTO users VALUES (1)", false)]
        #[case("with cte as (select 1) update users set name = 'x'", false)]
        // Plain DML (rejected)
        #[case("INSERT INTO users VALUES (1)", false)]
        #[case("  insert into users (id) values (1)", false)]
        #[case("UPDATE users SET name = 'new'", false)]
        #[case("  update users set active = true", false)]
        #[case("DELETE FROM users WHERE id = 1", false)]
        #[case("  delete from users", false)]
        // DDL (rejected)
        #[case("CREATE TABLE foo (id INT)", false)]
        #[case("DROP TABLE users", false)]
        #[case("ALTER TABLE users ADD COLUMN foo INT", false)]
        #[case("TRUNCATE users", false)]
        // Empty/whitespace
        #[case("", false)]
        #[case("   ", false)]
        // Parentheses after CTE
        #[case("WITH cte AS (SELECT 1) SELECT (1+2)", true)]
        #[case("WITH cte AS (SELECT 1) SELECT (SELECT 1)", true)]
        // String literals containing parentheses
        #[case("WITH cte AS (SELECT '(' FROM t) SELECT * FROM cte", true)]
        #[case("SELECT * FROM t WHERE name = '(test)'", true)]
        // Escaped quotes in strings
        #[case("SELECT * FROM t WHERE name = 'it''s'", true)]
        #[case("WITH cte AS (SELECT 'a''b') SELECT * FROM cte", true)]
        // SQL keywords inside string literals
        #[case("SELECT * FROM t WHERE action = 'delete'", true)]
        #[case("SELECT * FROM t WHERE cmd = 'INSERT INTO'", true)]
        // Identifiers containing keywords (word boundary test)
        #[case("SELECT mydelete FROM t", true)]
        #[case("SELECT delete_flag FROM t", true)]
        #[case("WITH mydelete AS (SELECT 1) SELECT * FROM mydelete", true)]
        #[case("SELECT * FROM users_to_delete", true)]
        // SQL comments containing keywords
        #[case("-- delete old records\nSELECT * FROM t", true)]
        #[case("/* update cache */ SELECT * FROM t", true)]
        #[case("SELECT * FROM t -- insert comment", true)]
        #[case("SELECT /* delete */ * FROM t", true)]
        // Non-ASCII characters (UTF-8 safety test)
        #[case("SELECT * FROM \"ユーザー\"", true)]
        #[case("SELECT name FROM users WHERE name = '日本語'", true)]
        #[case("WITH cte AS (SELECT '中文') SELECT * FROM cte", true)]
        // Multiple statements (rejected)
        #[case("SELECT 1; DELETE FROM users", false)]
        #[case("SELECT * FROM t; UPDATE t SET x = 1", false)]
        #[case("SELECT 1; SELECT 2", false)]
        // Semicolon in string is OK
        #[case("SELECT * FROM t WHERE x = ';'", true)]
        // Trailing semicolon is OK
        #[case("SELECT * FROM users;", true)]
        #[case("SELECT 1;", true)]
        #[case("SELECT * FROM t WHERE x = 1;", true)]
        #[case("WITH cte AS (SELECT 1) SELECT * FROM cte;", true)]
        // SELECT INTO (rejected - creates table)
        #[case("SELECT * INTO new_table FROM old_table", false)]
        #[case("SELECT id, name INTO backup FROM users", false)]
        // INTO in subquery is OK
        #[case("SELECT * FROM (SELECT 1) AS sub", true)]
        // INTO in string is OK
        #[case("SELECT * FROM t WHERE x = 'INTO'", true)]
        // CREATE TABLE AS SELECT (rejected)
        #[case("CREATE TABLE t AS SELECT * FROM users", false)]
        #[case("CREATE TABLE backup AS SELECT id FROM users", false)]
        // Writable CTE: DML inside CTE body (rejected)
        #[case(
            "WITH x AS (UPDATE users SET name='a' RETURNING *) SELECT * FROM x",
            false
        )]
        #[case("WITH x AS (DELETE FROM users RETURNING *) SELECT * FROM x", false)]
        // DML keyword inside double-quoted identifier (allowed — not real DML)
        #[case("SELECT \"update\" FROM t", true)]
        #[case("WITH x AS (SELECT 1 AS \"delete\") SELECT * FROM x", true)]
        #[case("SELECT \"up\"\"date\" FROM t", true)]
        // DML keyword inside dollar-quoted string (allowed — not real DML)
        #[case("SELECT $$update$$ AS label", true)]
        #[case("SELECT $tag$delete from here$tag$ AS s", true)]
        #[case("SELECT $$semi;colon$$ AS label", true)]
        // Unterminated dollar-quote currently consumes until end
        #[case("SELECT $$unclosed", true)]
        // Trailing semicolon + whitespace is still a single statement
        #[case("SELECT 1;   ", true)]
        // Semicolon followed by a comment is treated as additional content
        #[case("SELECT 1; -- done", false)]
        // Non-nested block comment parsing: leaked tokens after the first */
        // can appear, and rejected keywords there are conservatively blocked.
        #[case("SELECT /* outer /* inner */ still comment */ 1", true)]
        #[case("SELECT /* /* */ delete */ 1", false)]
        fn query_validation_returns_expected(#[case] query: &str, #[case] expected: bool) {
            assert_eq!(is_select_query(query), expected);
        }
    }
}
