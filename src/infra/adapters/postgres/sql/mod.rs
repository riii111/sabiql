fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(in crate::adapters::postgres) mod ddl;
pub(in crate::adapters::postgres) mod query;

#[cfg(test)]
mod tests {
    use super::quote_literal;
    use rstest::rstest;

    #[rstest]
    #[case::nul("a\0b", "'a\0b'")]
    #[case::newline("a\nb", "'a\nb'")]
    #[case::tab("a\tb", "'a\tb'")]
    #[case::substitute("a\u{1a}b", "'a\u{1a}b'")]
    #[case::non_ascii("日本語", "'日本語'")]
    fn quote_literal_keeps_characters_other_than_quotes_verbatim(
        #[case] value: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(quote_literal(value), expected);
    }

    #[test]
    fn quote_literal_escapes_embedded_quotes() {
        assert_eq!(quote_literal("O'Reilly"), "'O''Reilly'");
    }

    #[test]
    fn quote_literal_supports_only_quotes() {
        assert_eq!(quote_literal("''"), "''''''");
    }

    #[test]
    fn quote_literal_supports_empty_string() {
        assert_eq!(quote_literal(""), "''");
    }
}
