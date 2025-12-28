/// SQL identifier and literal quoting utilities for PostgreSQL.
///
/// These functions follow PostgreSQL's quoting rules for safe SQL interpolation.
/// Quote identifier for safe SQL representation (PostgreSQL style).
/// Doubles any embedded double quotes and wraps in double quotes.
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Escape string literal for safe SQL interpolation (PostgreSQL quote_literal equivalent).
/// Doubles any embedded single quotes and wraps in single quotes.
pub fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_ident_simple() {
        assert_eq!(quote_ident("users"), "\"users\"");
    }

    #[test]
    fn quote_ident_with_double_quote() {
        assert_eq!(quote_ident("user\"name"), "\"user\"\"name\"");
    }

    #[test]
    fn quote_ident_empty() {
        assert_eq!(quote_ident(""), "\"\"");
    }

    #[test]
    fn quote_literal_simple() {
        assert_eq!(quote_literal("hello"), "'hello'");
    }

    #[test]
    fn quote_literal_with_single_quote() {
        assert_eq!(quote_literal("it's"), "'it''s'");
    }

    #[test]
    fn quote_literal_multiple_quotes() {
        assert_eq!(quote_literal("a'b'c"), "'a''b''c'");
    }

    #[test]
    fn quote_literal_empty() {
        assert_eq!(quote_literal(""), "''");
    }
}
