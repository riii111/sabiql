// -- SQL lexical skip helpers (byte-level) --
// Shared by split_sql_statements and has_select_into to ensure
// consistent quote/comment boundary handling.

pub(super) fn skip_single_quoted(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            i += 1;
            if i < bytes.len() && bytes[i] == b'\'' {
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

pub(super) fn skip_double_quoted(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i += 1;
            if i < bytes.len() && bytes[i] == b'"' {
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

pub(super) fn skip_dollar_quoted(sql: &str, bytes: &[u8], mut i: usize) -> usize {
    let tag_start = i;
    i += 1;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'$' {
        let tag = &sql[tag_start..=i];
        i += 1;
        while i + tag.len() <= bytes.len() {
            if &sql[i..i + tag.len()] == tag {
                return i + tag.len();
            }
            i += 1;
        }
    }
    i
}

pub(super) fn skip_line_comment(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

pub(super) fn skip_block_comment(bytes: &[u8], mut i: usize) -> usize {
    i += 2;
    while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
        i += 1;
    }
    if i + 1 < bytes.len() {
        i += 2;
    }
    i
}

pub(in crate::infra::adapters::postgres) fn split_sql_statements(sql: &str) -> Vec<&str> {
    let bytes = sql.as_bytes();
    let mut stmts = Vec::new();
    let mut start = 0;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'\'' => i = skip_single_quoted(bytes, i),
            b'"' => i = skip_double_quoted(bytes, i),
            b'$' => i = skip_dollar_quoted(sql, bytes, i),
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => i = skip_line_comment(bytes, i),
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => i = skip_block_comment(bytes, i),
            b';' => {
                let slice = sql[start..i].trim();
                if !slice.is_empty() {
                    stmts.push(slice);
                }
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    let tail = sql[start..].trim();
    if !tail.is_empty() {
        stmts.push(tail);
    }
    stmts
}

pub(super) fn has_select_into(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut found_from = false;

    while i < bytes.len() {
        match bytes[i] {
            b'\'' => i = skip_single_quoted(bytes, i),
            b'"' => i = skip_double_quoted(bytes, i),
            b'$' => i = skip_dollar_quoted(lower, bytes, i),
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => i = skip_line_comment(bytes, i),
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => i = skip_block_comment(bytes, i),
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
                let word = &lower[word_start..i];
                match word {
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

#[cfg(test)]
mod tests {
    use rstest::rstest;

    mod split_sql_statements {
        use super::*;

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
        fn valid_input_returns_split_statements(#[case] sql: &str, #[case] expected: Vec<&str>) {
            assert_eq!(super::super::split_sql_statements(sql), expected);
        }

        #[rstest]
        #[case::single_quotes("SELECT 'a;b'", vec!["SELECT 'a;b'"])]
        #[case::double_quotes(r#"SELECT "a;b""#, vec![r#"SELECT "a;b""#])]
        #[case::dollar_quote("SELECT $$a;b$$", vec!["SELECT $$a;b$$"])]
        #[case::tagged_dollar_quote("SELECT $tag$a;b$tag$", vec!["SELECT $tag$a;b$tag$"])]
        #[case::line_comment("SELECT 1 -- comment; here\n", vec!["SELECT 1 -- comment; here"])]
        #[case::block_comment("SELECT /* ; */ 1", vec!["SELECT /* ; */ 1"])]
        fn quoted_semicolon_returns_single_statement(
            #[case] sql: &str,
            #[case] expected: Vec<&str>,
        ) {
            assert_eq!(super::super::split_sql_statements(sql), expected);
        }

        #[rstest]
        #[case::empty("")]
        #[case::whitespace_only("   ")]
        fn blank_input_returns_empty(#[case] sql: &str) {
            assert!(super::super::split_sql_statements(sql).is_empty());
        }
    }
}
