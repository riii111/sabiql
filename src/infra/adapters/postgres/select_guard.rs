/// PostgreSQL allows DML inside CTEs (e.g., `WITH ... UPDATE`), so we can't
/// just check if query starts with SELECT/WITH. We need to find the first
/// top-level SQL verb outside of parentheses, string literals, and comments.
/// Also rejects multiple statements and SELECT INTO (which creates tables).
pub(in crate::infra::adapters::postgres) fn is_select_query(query: &str) -> bool {
    let lower = query.trim().to_lowercase();
    let chars: Vec<(usize, char)> = lower.char_indices().collect();
    let len = chars.len();

    let mut i = 0;
    let mut depth = 0;
    let mut in_string = false;
    let mut found_select = false;

    while i < len {
        let (byte_pos, c) = chars[i];

        if c == '-' && i + 1 < len && chars[i + 1].1 == '-' {
            while i < len && chars[i].1 != '\n' {
                i += 1;
            }
            continue;
        }

        if c == '/' && i + 1 < len && chars[i + 1].1 == '*' {
            i += 2;
            while i + 1 < len && !(chars[i].1 == '*' && chars[i + 1].1 == '/') {
                i += 1;
            }
            i += 2; // skip */
            continue;
        }

        if c == '\'' {
            if in_string {
                if i + 1 < len && chars[i + 1].1 == '\'' {
                    i += 2; // escaped quote ''
                    continue;
                }
                in_string = false;
            } else {
                in_string = true;
            }
            i += 1;
            continue;
        }

        if in_string {
            i += 1;
            continue;
        }

        // Skip double-quoted identifiers ("column", "update", etc.)
        if c == '"' {
            i += 1;
            while i < len {
                if chars[i].1 == '"' {
                    if i + 1 < len && chars[i + 1].1 == '"' {
                        i += 2; // escaped "" inside identifier
                    } else {
                        i += 1; // closing quote
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            continue;
        }

        // Skip dollar-quoted strings ($$...$$, $tag$...$tag$)
        if c == '$' {
            let tag_start = byte_pos;
            let mut j = i + 1;
            while j < len && (chars[j].1.is_alphanumeric() || chars[j].1 == '_') {
                j += 1;
            }
            if j < len && chars[j].1 == '$' {
                let tag = &lower[tag_start..=chars[j].0];
                j += 1;
                while j + tag.len() <= len {
                    let candidate_start = chars[j].0;
                    if chars[j].1 == '$' {
                        let candidate_end = candidate_start + tag.len();
                        if candidate_end <= lower.len()
                            && &lower[candidate_start..candidate_end] == tag
                        {
                            let mut k = j;
                            while k < len && chars[k].0 < candidate_end {
                                k += 1;
                            }
                            j = k;
                            break;
                        }
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
        }

        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
        }

        // Reject multiple statements (but allow trailing semicolon)
        if depth == 0 && c == ';' {
            let remaining = &lower[byte_pos + 1..];
            if !remaining.trim().is_empty() {
                return false;
            }
            break;
        }

        if is_word_start(&chars, i) {
            let rest = &lower[byte_pos..];
            if depth == 0 && is_keyword(rest, "select") {
                found_select = true;
            }
            // SELECT INTO creates a table, reject it
            if depth == 0 && is_keyword(rest, "into") && found_select {
                return false;
            }
            // Reject DML/DDL at any depth (including inside CTEs)
            if is_keyword(rest, "insert")
                || is_keyword(rest, "update")
                || is_keyword(rest, "delete")
                || is_keyword(rest, "create")
            {
                return false;
            }
        }

        i += 1;
    }

    found_select
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
        // DML keyword inside dollar-quoted string (allowed — not real DML)
        #[case("SELECT $$update$$ AS label", true)]
        #[case("SELECT $tag$delete from here$tag$ AS s", true)]
        fn query_validation_returns_expected(#[case] query: &str, #[case] expected: bool) {
            assert_eq!(is_select_query(query), expected);
        }
    }
}
