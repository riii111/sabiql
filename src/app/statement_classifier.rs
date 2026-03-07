/// SQL statement kind. Based on PostgreSQL lexical rules.
/// May move behind a Port + Adapter boundary if MySQL support is added.
///
/// - `Unsupported`: a recognizable SQL command that this classifier does not yet classify
///   (e.g. GRANT, COPY, DO). SAB-100 will assign its risk level.
/// - `Other`: no statement keyword was found; callers must treat this as HIGH / blocked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementKind {
    Select,
    Insert,
    Update { has_where: bool },
    Delete { has_where: bool },
    Create,
    Alter,
    Drop,
    Truncate,
    Transaction,
    Unsupported,
    Other,
}

/// Classifies a SQL statement. Falls back to `Other` when ambiguous.
/// Multi-statement input (`;` separated) returns `Other` (SAB-102 scope).
pub fn classify(sql: &str) -> StatementKind {
    let lower = sql.trim().to_lowercase();
    if lower.is_empty() {
        return StatementKind::Other;
    }
    let chars: Vec<(usize, char)> = lower.char_indices().collect();
    classify_inner(&lower, &chars)
}

/// Extracts the target table name for high-risk confirmation.
/// Returns `None` when extraction fails or the statement targets multiple tables,
/// both of which callers must treat as blocked.
pub fn extract_table_name(sql: &str, kind: &StatementKind) -> Option<String> {
    let original_trimmed = sql.trim();
    // Avoids byte-length mismatch when Unicode identifiers change size under case folding.
    let chars: Vec<(usize, char)> = original_trimmed.char_indices().collect();

    match kind {
        StatementKind::Drop => extract_drop_table_name(original_trimmed, &chars),
        StatementKind::Truncate => extract_truncate_table_name(original_trimmed, &chars),
        StatementKind::Delete { .. } => extract_delete_table_name(original_trimmed, &chars),
        StatementKind::Update { .. } => extract_update_table_name(original_trimmed, &chars),
        _ => None,
    }
}

// Copied from select_guard.rs; intentionally not shared because
// select_guard.rs will be deleted in SAB-100.

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
    Some(cursor)
}

fn update_parentheses_depth(ch: char, depth: &mut i32) {
    if ch == '(' {
        *depth += 1;
    } else if ch == ')' {
        *depth -= 1;
    }
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

fn classify_inner(lower: &str, chars: &[(usize, char)]) -> StatementKind {
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut in_cte = false;
    let mut is_explain = false;
    let mut kind: Option<StatementKind> = None;

    while i < chars.len() {
        let (byte_pos, ch) = chars[i];

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
        if let Some(next_i) = skip_dollar_quoted_string(lower, chars, i, byte_pos, ch) {
            i = next_i;
            continue;
        }

        update_parentheses_depth(ch, &mut depth);

        // Multi-statement is SAB-102 scope; reject here for safety
        if depth == 0 && ch == ';' {
            if has_non_whitespace_after(lower, byte_pos) {
                return StatementKind::Other;
            }
            break;
        }

        if depth == 0 && (ch.is_alphabetic() || ch == '_') && is_word_start(chars, i) {
            let rest = &lower[byte_pos..];

            if kind.is_none() && is_keyword(rest, "explain") {
                is_explain = true;
                i += 1;
                continue;
            }

            if is_explain && kind.is_none() && match_keyword(rest).is_none() {
                i += 1;
                continue;
            }

            // SELECT INTO creates a table; treat as Other to avoid silent execution.
            if matches!(kind, Some(StatementKind::Select)) && is_keyword(rest, "into") {
                return StatementKind::Other;
            }

            if kind.is_none() && is_keyword(rest, "with") {
                in_cte = true;
                i += 1;
                continue;
            }

            // CTE: keep overriding with each top-level keyword (last one wins).
            // Non-CTE: first keyword determines the kind.
            if in_cte || kind.is_none() {
                if let Some(k) = match_keyword(rest) {
                    kind = Some(k);
                    if matches!(
                        kind,
                        Some(StatementKind::Update { .. } | StatementKind::Delete { .. })
                    ) {
                        let has_where = scan_for_where(lower, chars, i);
                        kind = Some(match kind.unwrap() {
                            StatementKind::Update { .. } => StatementKind::Update { has_where },
                            StatementKind::Delete { .. } => StatementKind::Delete { has_where },
                            other => other,
                        });
                    }
                } else if !in_cte {
                    kind = Some(StatementKind::Unsupported);
                }
            }
        }

        i += 1;
    }

    kind.unwrap_or(StatementKind::Other)
}

fn match_keyword(rest: &str) -> Option<StatementKind> {
    if is_keyword(rest, "select") {
        Some(StatementKind::Select)
    } else if is_keyword(rest, "insert") {
        Some(StatementKind::Insert)
    } else if is_keyword(rest, "update") {
        Some(StatementKind::Update { has_where: false })
    } else if is_keyword(rest, "delete") {
        Some(StatementKind::Delete { has_where: false })
    } else if is_keyword(rest, "create") {
        Some(StatementKind::Create)
    } else if is_keyword(rest, "alter") {
        Some(StatementKind::Alter)
    } else if is_keyword(rest, "drop") {
        Some(StatementKind::Drop)
    } else if is_keyword(rest, "truncate") {
        Some(StatementKind::Truncate)
    } else if is_keyword(rest, "begin")
        || is_keyword(rest, "commit")
        || is_keyword(rest, "rollback")
        || is_keyword(rest, "savepoint")
        || is_keyword(rest, "release")
    {
        Some(StatementKind::Transaction)
    } else if is_keyword(rest, "start") {
        if rest.len() > 6 {
            let after_start = rest[6..].trim_start();
            if is_keyword(after_start, "transaction") {
                return Some(StatementKind::Transaction);
            }
        }
        None
    } else if is_keyword(rest, "show") {
        Some(StatementKind::Select)
    } else {
        None
    }
}

fn scan_for_where(lower: &str, chars: &[(usize, char)], start: usize) -> bool {
    let mut i = start;
    let mut depth: i32 = 0;
    let mut in_string = false;

    while i < chars.len() {
        let (byte_pos, ch) = chars[i];

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
        if let Some(next_i) = skip_dollar_quoted_string(lower, chars, i, byte_pos, ch) {
            i = next_i;
            continue;
        }

        update_parentheses_depth(ch, &mut depth);

        if depth == 0 && is_word_start(chars, i) {
            let rest = &lower[byte_pos..];
            if is_keyword(rest, "where") {
                return true;
            }
        }

        i += 1;
    }

    false
}

fn has_non_whitespace_after(lower: &str, byte_pos: usize) -> bool {
    lower
        .get(byte_pos + 1..)
        .map(|tail| !tail.trim().is_empty())
        .unwrap_or(false)
}

/// Collects top-level tokens (original case). Commas are captured as `","` entries.
fn collect_top_level_tokens(original: &str, chars: &[(usize, char)]) -> Vec<(usize, String)> {
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut in_string = false;

    while i < chars.len() {
        let (byte_pos, ch) = chars[i];

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

        if ch == '"' {
            let start_i = i;
            if let Some(next_i) = skip_double_quoted_identifier(chars, i, ch) {
                if depth == 0 {
                    let start_byte = chars[start_i].0;
                    let end_byte = if next_i < chars.len() {
                        chars[next_i].0
                    } else {
                        original.len()
                    };
                    tokens.push((start_byte, original[start_byte..end_byte].to_string()));
                }
                i = next_i;
                continue;
            }
        }

        if let Some(next_i) = skip_dollar_quoted_string(original, chars, i, byte_pos, ch) {
            i = next_i;
            continue;
        }

        update_parentheses_depth(ch, &mut depth);

        if depth == 0 && ch == ',' {
            tokens.push((byte_pos, ",".to_string()));
            i += 1;
            continue;
        }

        if depth == 0 && (ch.is_alphanumeric() || ch == '_') && is_word_start(chars, i) {
            let start_byte = byte_pos;
            let mut end_i = i;
            while end_i + 1 < chars.len()
                && (chars[end_i + 1].1.is_alphanumeric() || chars[end_i + 1].1 == '_')
            {
                end_i += 1;
            }
            let end_byte = if end_i + 1 < chars.len() {
                chars[end_i + 1].0
            } else {
                original.len()
            };
            tokens.push((start_byte, original[start_byte..end_byte].to_string()));
            i = end_i + 1;
            continue;
        }

        if depth == 0 && ch == '.' && !tokens.is_empty() {
            let prev = tokens.last_mut().unwrap();
            prev.1.push('.');
            i += 1;
            if i < chars.len() {
                let (next_byte, next_ch) = chars[i];
                if next_ch == '"' {
                    if let Some(next_i) = skip_double_quoted_identifier(chars, i, next_ch) {
                        let end_byte = if next_i < chars.len() {
                            chars[next_i].0
                        } else {
                            original.len()
                        };
                        prev.1.push_str(&original[next_byte..end_byte]);
                        i = next_i;
                        continue;
                    }
                } else if next_ch.is_alphanumeric() || next_ch == '_' {
                    let mut end_i = i;
                    while end_i + 1 < chars.len()
                        && (chars[end_i + 1].1.is_alphanumeric() || chars[end_i + 1].1 == '_')
                    {
                        end_i += 1;
                    }
                    let end_byte = if end_i + 1 < chars.len() {
                        chars[end_i + 1].0
                    } else {
                        original.len()
                    };
                    prev.1.push_str(&original[next_byte..end_byte]);
                    i = end_i + 1;
                    continue;
                }
            }
            continue;
        }

        i += 1;
    }

    tokens
}

fn unquote_simple(name: &str) -> String {
    if let Some(dot_pos) = find_unquoted_dot(name) {
        let schema = &name[..dot_pos];
        let table = &name[dot_pos + 1..];
        let schema_unquoted = unquote_single_ident(schema);
        let table_unquoted = unquote_single_ident(table);
        return format!("{schema_unquoted}.{table_unquoted}");
    }
    unquote_single_ident(name)
}

fn find_unquoted_dot(name: &str) -> Option<usize> {
    let mut in_quote = false;
    for (i, ch) in name.char_indices() {
        if ch == '"' {
            in_quote = !in_quote;
        } else if ch == '.' && !in_quote {
            return Some(i);
        }
    }
    None
}

fn unquote_single_ident(s: &str) -> String {
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].replace("\"\"", "\"")
    } else {
        s.to_string()
    }
}

fn extract_drop_table_name(original: &str, chars: &[(usize, char)]) -> Option<String> {
    let tokens = collect_top_level_tokens(original, chars);
    let lowers: Vec<String> = tokens.iter().map(|(_, t)| t.to_lowercase()).collect();

    let drop_idx = lowers.iter().position(|t| t == "drop")?;
    let table_idx = lowers.get(drop_idx + 1).and_then(|t| {
        if t == "table" {
            Some(drop_idx + 1)
        } else {
            None
        }
    })?;

    let mut name_idx = table_idx + 1;

    if lowers.get(name_idx).map(|t| t.as_str()) == Some("if")
        && lowers.get(name_idx + 1).map(|t| t.as_str()) == Some("exists")
    {
        name_idx += 2;
    }

    let raw = tokens.get(name_idx).map(|(_, t)| t.as_str())?;
    if tokens.get(name_idx + 1).map(|(_, t)| t.as_str()) == Some(",") {
        return None;
    }
    Some(unquote_simple(raw))
}

fn extract_truncate_table_name(original: &str, chars: &[(usize, char)]) -> Option<String> {
    let tokens = collect_top_level_tokens(original, chars);
    let lowers: Vec<String> = tokens.iter().map(|(_, t)| t.to_lowercase()).collect();

    let trunc_idx = lowers.iter().position(|t| t == "truncate")?;
    let mut name_idx = trunc_idx + 1;

    if lowers.get(name_idx).map(|t| t.as_str()) == Some("table") {
        name_idx += 1;
    }

    if lowers.get(name_idx).map(|t| t.as_str()) == Some("only") {
        name_idx += 1;
    }

    let raw = tokens.get(name_idx).map(|(_, t)| t.as_str())?;
    if tokens.get(name_idx + 1).map(|(_, t)| t.as_str()) == Some(",") {
        return None;
    }
    Some(unquote_simple(raw))
}

fn extract_delete_table_name(original: &str, chars: &[(usize, char)]) -> Option<String> {
    let tokens = collect_top_level_tokens(original, chars);
    let lowers: Vec<String> = tokens.iter().map(|(_, t)| t.to_lowercase()).collect();

    let delete_idx = lowers.iter().position(|t| t == "delete")?;

    if lowers.get(delete_idx + 1).map(|t| t.as_str()) != Some("from") {
        return None;
    }

    let mut name_idx = delete_idx + 2;

    if lowers.get(name_idx).map(|t| t.as_str()) == Some("only") {
        name_idx += 1;
    }

    let raw = tokens.get(name_idx).map(|(_, t)| t.as_str())?;
    Some(unquote_simple(raw))
}

fn extract_update_table_name(original: &str, chars: &[(usize, char)]) -> Option<String> {
    let tokens = collect_top_level_tokens(original, chars);
    let lowers: Vec<String> = tokens.iter().map(|(_, t)| t.to_lowercase()).collect();

    let update_idx = lowers.iter().position(|t| t == "update")?;

    let mut name_idx = update_idx + 1;

    if lowers.get(name_idx).map(|t| t.as_str()) == Some("only") {
        name_idx += 1;
    }

    let raw = tokens.get(name_idx).map(|(_, t)| t.as_str())?;
    Some(unquote_simple(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod classify_tests {
        use super::*;

        #[rstest]
        #[case::plain_select("SELECT * FROM users", StatementKind::Select)]
        #[case::lowercase_select("select id from users", StatementKind::Select)]
        #[case::cte_select("WITH cte AS (SELECT 1) SELECT * FROM cte", StatementKind::Select)]
        #[case::recursive_cte_select(
            "WITH RECURSIVE tree AS (SELECT 1) SELECT * FROM tree",
            StatementKind::Select
        )]
        #[case::multiple_ctes_select(
            "WITH a AS (SELECT 1), b AS (SELECT 2) SELECT * FROM a, b",
            StatementKind::Select
        )]
        #[case::explain_select("EXPLAIN SELECT * FROM users", StatementKind::Select)]
        #[case::explain_analyze_select(
            "EXPLAIN ANALYZE SELECT * FROM users",
            StatementKind::Select
        )]
        #[case::explain_verbose_select(
            "EXPLAIN VERBOSE SELECT * FROM users",
            StatementKind::Select
        )]
        #[case::explain_costs_off("EXPLAIN COSTS OFF SELECT * FROM users", StatementKind::Select)]
        #[case::show("SHOW search_path", StatementKind::Select)]
        #[case::show_all("SHOW ALL", StatementKind::Select)]
        fn select_variants(#[case] sql: &str, #[case] expected: StatementKind) {
            assert_eq!(classify(sql), expected);
        }

        #[rstest]
        #[case::explain_analyze_update(
            "EXPLAIN ANALYZE UPDATE users SET name = 'x'",
            StatementKind::Update { has_where: false }
        )]
        #[case::explain_analyze_update_where(
            "EXPLAIN ANALYZE UPDATE users SET name = 'x' WHERE id = 1",
            StatementKind::Update { has_where: true }
        )]
        #[case::explain_analyze_delete(
            "EXPLAIN ANALYZE DELETE FROM users",
            StatementKind::Delete { has_where: false }
        )]
        #[case::explain_analyze_insert(
            "EXPLAIN ANALYZE INSERT INTO users VALUES (1)",
            StatementKind::Insert
        )]
        fn explain_analyze_dml(#[case] sql: &str, #[case] expected: StatementKind) {
            assert_eq!(classify(sql), expected);
        }

        #[rstest]
        #[case::cte_update(
            "WITH cte AS (SELECT 1) UPDATE users SET name = 'x'",
            StatementKind::Update { has_where: false }
        )]
        #[case::cte_delete(
            "WITH cte AS (SELECT 1) DELETE FROM users",
            StatementKind::Delete { has_where: false }
        )]
        #[case::cte_delete_where(
            "WITH cte AS (SELECT 1) DELETE FROM users WHERE id = 1",
            StatementKind::Delete { has_where: true }
        )]
        fn cte_dml(#[case] sql: &str, #[case] expected: StatementKind) {
            assert_eq!(classify(sql), expected);
        }

        #[rstest]
        #[case::plain_insert("INSERT INTO users VALUES (1)", StatementKind::Insert)]
        #[case::update_no_where("UPDATE users SET name = 'x'", StatementKind::Update { has_where: false })]
        #[case::update_with_where("UPDATE users SET name = 'x' WHERE id = 1", StatementKind::Update { has_where: true })]
        #[case::delete_no_where("DELETE FROM users", StatementKind::Delete { has_where: false })]
        #[case::delete_with_where("DELETE FROM users WHERE id = 1", StatementKind::Delete { has_where: true })]
        fn dml(#[case] sql: &str, #[case] expected: StatementKind) {
            assert_eq!(classify(sql), expected);
        }

        #[rstest]
        #[case::create_table("CREATE TABLE foo (id INT)", StatementKind::Create)]
        #[case::alter_table("ALTER TABLE users ADD COLUMN foo INT", StatementKind::Alter)]
        #[case::drop_table("DROP TABLE users", StatementKind::Drop)]
        #[case::truncate("TRUNCATE users", StatementKind::Truncate)]
        fn ddl(#[case] sql: &str, #[case] expected: StatementKind) {
            assert_eq!(classify(sql), expected);
        }

        #[rstest]
        #[case::begin("BEGIN", StatementKind::Transaction)]
        #[case::commit("COMMIT", StatementKind::Transaction)]
        #[case::rollback("ROLLBACK", StatementKind::Transaction)]
        #[case::savepoint("SAVEPOINT sp1", StatementKind::Transaction)]
        #[case::start_transaction("START TRANSACTION", StatementKind::Transaction)]
        #[case::rollback_to_savepoint("ROLLBACK TO SAVEPOINT sp1", StatementKind::Transaction)]
        #[case::release_savepoint("RELEASE SAVEPOINT sp1", StatementKind::Transaction)]
        fn transaction(#[case] sql: &str, #[case] expected: StatementKind) {
            assert_eq!(classify(sql), expected);
        }

        #[rstest]
        #[case::grant("GRANT SELECT ON users TO role1", StatementKind::Unsupported)]
        #[case::revoke("REVOKE SELECT ON users FROM role1", StatementKind::Unsupported)]
        #[case::copy("COPY users FROM '/tmp/data.csv'", StatementKind::Unsupported)]
        #[case::do_block("DO $$ BEGIN RAISE NOTICE 'hi'; END $$", StatementKind::Unsupported)]
        #[case::call("CALL my_procedure()", StatementKind::Unsupported)]
        #[case::merge(
            "MERGE INTO t USING s ON t.id = s.id WHEN MATCHED THEN UPDATE SET x = 1",
            StatementKind::Unsupported
        )]
        fn unsupported(#[case] sql: &str, #[case] expected: StatementKind) {
            assert_eq!(classify(sql), expected);
        }

        #[rstest]
        #[case::empty("", StatementKind::Other)]
        #[case::whitespace_only("   ", StatementKind::Other)]
        #[case::comment_only("-- just a comment", StatementKind::Other)]
        #[case::block_comment_only("/* nothing */", StatementKind::Other)]
        fn other(#[case] sql: &str, #[case] expected: StatementKind) {
            assert_eq!(classify(sql), expected);
        }

        #[rstest]
        #[case::string_literal_keyword(
            "SELECT * FROM t WHERE action = 'delete'",
            StatementKind::Select
        )]
        #[case::double_quoted_keyword("SELECT \"update\" FROM t", StatementKind::Select)]
        #[case::dollar_quoted_keyword("SELECT $$delete$$ AS label", StatementKind::Select)]
        #[case::mixed_case("SeLeCt * FROM users", StatementKind::Select)]
        #[case::leading_comment("-- comment\nSELECT * FROM t", StatementKind::Select)]
        #[case::leading_block_comment("/* comment */ SELECT * FROM t", StatementKind::Select)]
        #[case::multiple_statements("SELECT 1; DELETE FROM users", StatementKind::Other)]
        #[case::trailing_semicolon("SELECT 1;", StatementKind::Select)]
        #[case::trailing_semicolon_whitespace("SELECT 1;   ", StatementKind::Select)]
        #[case::where_in_subquery(
            "DELETE FROM users WHERE id IN (SELECT id FROM old_users)",
            StatementKind::Delete { has_where: true }
        )]
        #[case::where_in_string_not_real(
            "UPDATE users SET note = 'where is my cat'",
            StatementKind::Update { has_where: false }
        )]
        #[case::where_in_dollar_quote(
            "UPDATE users SET note = $$where$$ WHERE id = 1",
            StatementKind::Update { has_where: true }
        )]
        #[case::tagged_dollar_quote_keyword(
            "SELECT $tag$delete from here$tag$ AS s",
            StatementKind::Select
        )]
        #[case::semicolon_inside_string("SELECT * FROM t WHERE x = ';'", StatementKind::Select)]
        #[case::semicolon_in_dollar_quote("SELECT $$semi;colon$$ AS label", StatementKind::Select)]
        #[case::non_ascii_identifier("SELECT * FROM \"ユーザー\"", StatementKind::Select)]
        #[case::non_ascii_literal(
            "SELECT name FROM users WHERE name = '日本語'",
            StatementKind::Select
        )]
        #[case::unterminated_dollar_quote("SELECT $$unclosed", StatementKind::Select)]
        #[case::nested_block_comment(
            "SELECT /* outer /* inner */ still comment */ 1",
            StatementKind::Select
        )]
        #[case::identifier_contains_keyword("SELECT delete_flag FROM t", StatementKind::Select)]
        #[case::table_name_contains_keyword("SELECT * FROM users_to_delete", StatementKind::Select)]
        #[case::double_quoted_escaped("SELECT \"up\"\"date\" FROM t", StatementKind::Select)]
        #[case::cte_with_update_in_subquery(
            "WITH x AS (UPDATE users SET name='a' RETURNING *) SELECT * FROM x",
            StatementKind::Select
        )]
        #[case::select_with_parenthesized_expr(
            "WITH cte AS (SELECT 1) SELECT (1+2)",
            StatementKind::Select
        )]
        #[case::delete_then_select("DELETE FROM users; SELECT 1", StatementKind::Other)]
        #[case::select_then_select("SELECT 1; SELECT 2", StatementKind::Other)]
        #[case::select_into("SELECT * INTO backup FROM users", StatementKind::Other)]
        #[case::select_into_columns("SELECT id, name INTO backup FROM users", StatementKind::Other)]
        fn edge_cases(#[case] sql: &str, #[case] expected: StatementKind) {
            assert_eq!(classify(sql), expected);
        }
    }

    mod extract_table_name_tests {
        use super::*;

        #[rstest]
        #[case::simple("DROP TABLE users", StatementKind::Drop, Some("users"))]
        #[case::if_exists("DROP TABLE IF EXISTS users", StatementKind::Drop, Some("users"))]
        #[case::schema_qualified(
            "DROP TABLE IF EXISTS public.users",
            StatementKind::Drop,
            Some("public.users")
        )]
        #[case::quoted("DROP TABLE \"Users\"", StatementKind::Drop, Some("Users"))]
        #[case::quoted_schema(
            "DROP TABLE \"public\".\"Users\"",
            StatementKind::Drop,
            Some("public.Users")
        )]
        fn drop_table(
            #[case] sql: &str,
            #[case] kind: StatementKind,
            #[case] expected: Option<&str>,
        ) {
            assert_eq!(
                extract_table_name(sql, &kind),
                expected.map(|s| s.to_string())
            );
        }

        #[rstest]
        #[case::simple("TRUNCATE users", StatementKind::Truncate, Some("users"))]
        #[case::with_table_keyword("TRUNCATE TABLE users", StatementKind::Truncate, Some("users"))]
        #[case::multiple("TRUNCATE a, b", StatementKind::Truncate, None)]
        fn truncate(
            #[case] sql: &str,
            #[case] kind: StatementKind,
            #[case] expected: Option<&str>,
        ) {
            assert_eq!(
                extract_table_name(sql, &kind),
                expected.map(|s| s.to_string())
            );
        }

        #[rstest]
        #[case::simple(
            "DELETE FROM users",
            StatementKind::Delete { has_where: false },
            Some("users")
        )]
        #[case::with_only(
            "DELETE FROM ONLY users",
            StatementKind::Delete { has_where: false },
            Some("users")
        )]
        #[case::schema_qualified(
            "DELETE FROM public.users",
            StatementKind::Delete { has_where: false },
            Some("public.users")
        )]
        fn delete(#[case] sql: &str, #[case] kind: StatementKind, #[case] expected: Option<&str>) {
            assert_eq!(
                extract_table_name(sql, &kind),
                expected.map(|s| s.to_string())
            );
        }

        #[rstest]
        #[case::simple(
            "UPDATE users SET name = 'x'",
            StatementKind::Update { has_where: false },
            Some("users")
        )]
        #[case::with_only(
            "UPDATE ONLY users SET name = 'x'",
            StatementKind::Update { has_where: false },
            Some("users")
        )]
        #[case::schema_qualified(
            "UPDATE public.users SET name = 'x'",
            StatementKind::Update { has_where: false },
            Some("public.users")
        )]
        fn update(#[case] sql: &str, #[case] kind: StatementKind, #[case] expected: Option<&str>) {
            assert_eq!(
                extract_table_name(sql, &kind),
                expected.map(|s| s.to_string())
            );
        }

        #[rstest]
        #[case::select("SELECT * FROM users", StatementKind::Select)]
        #[case::insert("INSERT INTO users VALUES (1)", StatementKind::Insert)]
        #[case::create("CREATE TABLE users (id INT)", StatementKind::Create)]
        #[case::alter("ALTER TABLE users ADD COLUMN x INT", StatementKind::Alter)]
        #[case::transaction("BEGIN", StatementKind::Transaction)]
        #[case::other("GRANT SELECT ON users TO role1", StatementKind::Other)]
        fn not_applicable(#[case] sql: &str, #[case] kind: StatementKind) {
            assert_eq!(extract_table_name(sql, &kind), None);
        }

        #[rstest]
        #[case::truncate_schema(
            "TRUNCATE TABLE public.events",
            StatementKind::Truncate,
            Some("public.events")
        )]
        #[case::truncate_only(
            "TRUNCATE TABLE ONLY partitioned_table",
            StatementKind::Truncate,
            Some("partitioned_table")
        )]
        #[case::delete_quoted(
            "DELETE FROM \"MyTable\"",
            StatementKind::Delete { has_where: false },
            Some("MyTable")
        )]
        #[case::update_quoted_schema(
            "UPDATE \"public\".\"MyTable\" SET x = 1",
            StatementKind::Update { has_where: false },
            Some("public.MyTable")
        )]
        #[case::drop_no_table_keyword("DROP INDEX my_index", StatementKind::Drop, None)]
        #[case::drop_multiple("DROP TABLE a, b", StatementKind::Drop, None)]
        #[case::truncate_multiple("TRUNCATE a, b, c", StatementKind::Truncate, None)]
        fn additional_extraction(
            #[case] sql: &str,
            #[case] kind: StatementKind,
            #[case] expected: Option<&str>,
        ) {
            assert_eq!(
                extract_table_name(sql, &kind),
                expected.map(|s| s.to_string())
            );
        }
    }
}
