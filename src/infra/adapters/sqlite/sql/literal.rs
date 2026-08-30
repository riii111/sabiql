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

        #[test]
        fn identifier_escapes_embedded_quotes() {
            assert_eq!(quote_ident(r#"my"table"#), r#""my""table""#);
        }

        #[test]
        fn literal_escapes_embedded_quotes() {
            assert_eq!(quote_literal("O'Reilly"), "'O''Reilly'");
        }
    }
}
