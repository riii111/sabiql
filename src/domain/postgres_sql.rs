pub fn split_statements(sql: &str) -> Vec<&str> {
    let bytes = sql.as_bytes();
    let mut statements = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut depth = 0;

    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'\'' | b'"') => i = skip_quoted(bytes, i, quote),
            b'$' => i = skip_dollar_quoted(sql, bytes, i),
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                i = skip_line_comment(bytes, i);
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i = skip_block_comment(bytes, i);
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            b';' if depth == 0 => {
                push_statement(sql, start, i, &mut statements);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }

    push_statement(sql, start, sql.len(), &mut statements);
    statements
}

pub fn has_select_into(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    let mut i = 0;
    let mut depth = 0;
    let mut found_from = false;

    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'\'' | b'"') => i = skip_quoted(bytes, i, quote),
            b'$' => i = skip_dollar_quoted(lower, bytes, i),
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                i = skip_line_comment(bytes, i);
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i = skip_block_comment(bytes, i);
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            _ if depth == 0 && bytes[i].is_ascii_alphabetic() => {
                let word_start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                match &lower[word_start..i] {
                    "from" => found_from = true,
                    "into" if !found_from => {
                        if word_start >= 6 && lower[..word_start].trim_end().ends_with("insert") {
                            continue;
                        }
                        return true;
                    }
                    _ => {}
                }
            }
            _ => i += 1,
        }
    }

    false
}

fn push_statement<'sql>(sql: &'sql str, start: usize, end: usize, statements: &mut Vec<&'sql str>) {
    let statement = sql[start..end].trim();
    if !statement.is_empty() && !is_comment_only(statement) {
        statements.push(statement);
    }
}

fn is_comment_only(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
        } else if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            i = skip_line_comment(bytes, i);
        } else if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i = skip_block_comment(bytes, i);
        } else {
            return false;
        }
    }

    true
}

fn skip_quoted(bytes: &[u8], mut i: usize, quote: u8) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            i += 1;
            if i < bytes.len() && bytes[i] == quote {
                i += 1;
            } else {
                break;
            }
        } else {
            i += 1;
        }
    }
    i
}

fn skip_dollar_quoted(sql: &str, bytes: &[u8], mut i: usize) -> usize {
    let tag_start = i;
    i += 1;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'$' {
        return i;
    }

    let tag = &sql[tag_start..=i];
    i += 1;
    while i + tag.len() <= bytes.len() {
        if sql.as_bytes()[i..].starts_with(tag.as_bytes()) {
            return i + tag.len();
        }
        i += 1;
    }
    bytes.len()
}

fn skip_line_comment(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn skip_block_comment(bytes: &[u8], mut i: usize) -> usize {
    i += 2;
    while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
        i += 1;
    }
    if i + 1 < bytes.len() {
        i += 2;
    }
    i
}

#[cfg(test)]
mod tests {
    mod statement_scanner {
        use crate::postgres_sql::split_statements;
        use rstest::rstest;

        #[rstest]
        #[case::two_statements("SELECT 1; SELECT 2", vec!["SELECT 1", "SELECT 2"])]
        #[case::single_statement("SELECT 1", vec!["SELECT 1"])]
        #[case::trailing_semicolon("SELECT 1;", vec!["SELECT 1"])]
        #[case::multi_statement_txn(
            "BEGIN; CREATE TABLE t AS SELECT 1; COMMIT",
            vec!["BEGIN", "CREATE TABLE t AS SELECT 1", "COMMIT"]
        )]
        #[case::escaped_single_quote(
            "SELECT 'it''s'; SELECT 2",
            vec!["SELECT 'it''s'", "SELECT 2"]
        )]
        #[case::escaped_double_quote(
            "SELECT \"a\"\"b\"; SELECT 2",
            vec!["SELECT \"a\"\"b\"", "SELECT 2"]
        )]
        #[case::semicolon_in_line_comment(
            "SELECT 1 -- ;comment\n; SELECT 2",
            vec!["SELECT 1 -- ;comment", "SELECT 2"]
        )]
        #[case::semicolon_in_block_comment(
            "SELECT /* ; */ 1; SELECT 2",
            vec!["SELECT /* ; */ 1", "SELECT 2"]
        )]
        fn valid_input_returns_split_statements(#[case] sql: &str, #[case] expected: Vec<&str>) {
            assert_eq!(split_statements(sql), expected);
        }

        #[rstest]
        #[case::single_quotes("SELECT 'a;b'", vec!["SELECT 'a;b'"])]
        #[case::double_quotes(r#"SELECT "a;b""#, vec![r#"SELECT "a;b""#])]
        #[case::dollar_quote("SELECT $$a;b$$", vec!["SELECT $$a;b$$"])]
        #[case::tagged_dollar_quote("SELECT $tag$a;b$tag$", vec!["SELECT $tag$a;b$tag$"])]
        #[case::line_comment("SELECT 1 -- comment; here\n", vec!["SELECT 1 -- comment; here"])]
        #[case::block_comment("SELECT /* ; */ 1", vec!["SELECT /* ; */ 1"])]
        #[case::unterminated_single_quote("SELECT 'a;b", vec!["SELECT 'a;b"])]
        #[case::unterminated_double_quote("SELECT \"a;b", vec!["SELECT \"a;b"])]
        #[case::unterminated_dollar_quote("SELECT $$a;b", vec!["SELECT $$a;b"])]
        #[case::unicode_before_boundary("SELECT 'İ'; SELECT 2", vec!["SELECT 'İ'", "SELECT 2"])]
        fn quoted_semicolon_returns_single_statement(
            #[case] sql: &str,
            #[case] expected: Vec<&str>,
        ) {
            assert_eq!(split_statements(sql), expected);
        }

        #[test]
        fn dollar_quote_tag_is_case_sensitive() {
            let sql = "SELECT $Tag$a;b$tag$; SELECT 2";

            assert_eq!(split_statements(sql), vec![sql]);
        }

        #[rstest]
        #[case::empty("")]
        #[case::whitespace_only("   ")]
        #[case::comment_only("-- comment\n/* comment */")]
        fn blank_or_comment_only_input_returns_empty(#[case] sql: &str) {
            assert!(split_statements(sql).is_empty());
        }
    }
}
