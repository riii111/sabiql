mod command_tag;
mod lexer;
mod metadata;

pub(in crate::infra::adapters::postgres) use lexer::split_sql_statements;
