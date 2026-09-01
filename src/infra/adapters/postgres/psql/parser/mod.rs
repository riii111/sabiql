mod command_tag;
mod metadata;

pub(in crate::adapters::postgres) use crate::domain::postgres_sql::split_statements as split_sql_statements;
pub(in crate::adapters::postgres) use command_tag::ParseCommandTagError;
