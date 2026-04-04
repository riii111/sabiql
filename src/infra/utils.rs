pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_ident_returns_simple_quoted_identifier() {
        assert_eq!(quote_ident("users"), "\"users\"");
    }

    #[test]
    fn quote_ident_returns_escaped_double_quote() {
        assert_eq!(quote_ident("user\"name"), "\"user\"\"name\"");
    }

    #[test]
    fn quote_ident_returns_empty_quoted_identifier() {
        assert_eq!(quote_ident(""), "\"\"");
    }

    #[test]
    fn quote_literal_returns_simple_quoted_literal() {
        assert_eq!(quote_literal("hello"), "'hello'");
    }

    #[test]
    fn quote_literal_returns_escaped_single_quote() {
        assert_eq!(quote_literal("it's"), "'it''s'");
    }

    #[test]
    fn quote_literal_returns_multiple_escaped_quotes() {
        assert_eq!(quote_literal("a'b'c"), "'a''b''c'");
    }

    #[test]
    fn quote_literal_returns_empty_quoted_literal() {
        assert_eq!(quote_literal(""), "''");
    }
}
