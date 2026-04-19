fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(in crate::adapters::postgres) mod ddl;
pub(in crate::adapters::postgres) mod dialect;
pub(in crate::adapters::postgres) mod query;

#[cfg(test)]
mod tests {
    use super::{quote_ident, quote_literal};

    #[test]
    fn quote_ident_escapes_embedded_quotes() {
        assert_eq!(quote_ident(r#"user"name"#), r#""user""name""#);
    }

    #[test]
    fn quote_ident_supports_empty_string() {
        assert_eq!(quote_ident(""), "\"\"");
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
