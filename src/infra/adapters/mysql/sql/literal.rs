pub(in crate::adapters::mysql) fn quote_identifier(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}

pub(in crate::adapters::mysql) fn quote_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\0' => escaped.push_str("\\0"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\u{001a}' => escaped.push_str("\\Z"),
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            _ => escaped.push(character),
        }
    }
    format!("'{escaped}'")
}

#[cfg(test)]
mod tests {
    use super::{quote_identifier, quote_string};
    use rstest::rstest;

    #[rstest]
    #[case::plain("users", "`users`")]
    #[case::double_quote("a\"b", "`a\"b`")]
    #[case::backtick("a`b", "`a``b`")]
    #[case::empty("", "``")]
    fn identifiers_double_embedded_backticks(#[case] name: &str, #[case] expected: &str) {
        assert_eq!(quote_identifier(name), expected);
    }

    #[rstest]
    #[case::single_quote("O'Reilly", "'O\\'Reilly'")]
    #[case::backslash("C:\\temp", "'C:\\\\temp'")]
    #[case::non_ascii("日本語", "'日本語'")]
    fn strings_backslash_escape_quotes_and_backslashes(
        #[case] value: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(quote_string(value), expected);
    }

    #[rstest]
    #[case::nul("a\0b", "'a\\0b'")]
    #[case::newline("a\nb", "'a\\nb'")]
    #[case::carriage_return("a\rb", "'a\\rb'")]
    #[case::tab("a\tb", "'a\\tb'")]
    #[case::backspace("a\u{8}b", "'a\\bb'")]
    #[case::substitute("a\u{1a}b", "'a\\Zb'")]
    fn strings_backslash_escape_control_characters(#[case] value: &str, #[case] expected: &str) {
        assert_eq!(quote_string(value), expected);
    }
}
