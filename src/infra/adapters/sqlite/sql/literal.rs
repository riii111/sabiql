const SQLITE_NUL_TEXT_TRANSPORT_TAG: &str = "SABIQL_HEX:";

pub(in crate::adapters::sqlite) const PREVIEW_TRANSPORT_UNISTR_PREFIX: &str = "\\u0001SABIQL_HEX:";
pub(in crate::adapters::sqlite) fn sqlite_nul_text_sentinel() -> String {
    format!("\x01{SQLITE_NUL_TEXT_TRANSPORT_TAG}")
}

pub(super) fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

pub(super) fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(super) fn encode_preview_column_expr(column: &str) -> String {
    let ident = quote_ident(column);
    format!(
        "CASE WHEN typeof({ident}) = 'text' \
         THEN char(1) || '{SQLITE_NUL_TEXT_TRANSPORT_TAG}' || hex({ident}) \
         ELSE {ident} END AS {ident}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    mod quoting {
        use super::*;
        use rstest::rstest;

        #[test]
        fn identifier_escapes_embedded_quotes() {
            assert_eq!(quote_ident(r#"my"table"#), r#""my""table""#);
        }

        #[test]
        fn literal_escapes_embedded_quotes() {
            assert_eq!(quote_literal("O'Reilly"), "'O''Reilly'");
        }

        #[rstest]
        #[case::backtick("a`b", "\"a`b\"")]
        #[case::empty("", "\"\"")]
        #[case::non_ascii("日本語", "\"日本語\"")]
        fn identifier_keeps_other_characters_verbatim(#[case] name: &str, #[case] expected: &str) {
            assert_eq!(quote_ident(name), expected);
        }

        #[rstest]
        #[case::backslash("C:\\temp", "'C:\\temp'")]
        #[case::newline("a\nb", "'a\nb'")]
        #[case::tab("a\tb", "'a\tb'")]
        #[case::substitute("a\u{1a}b", "'a\u{1a}b'")]
        #[case::non_ascii("日本語", "'日本語'")]
        fn literal_keeps_characters_other_than_quotes_verbatim(
            #[case] value: &str,
            #[case] expected: &str,
        ) {
            assert_eq!(quote_literal(value), expected);
        }
    }
}
