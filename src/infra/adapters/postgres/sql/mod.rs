fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(in crate::infra::adapters::postgres) mod ddl;
pub(in crate::infra::adapters::postgres) mod dialect;
pub(in crate::infra::adapters::postgres) mod query;
