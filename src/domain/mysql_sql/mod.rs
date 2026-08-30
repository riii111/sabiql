mod classifier;
mod export;
mod lexer;
mod side_effect;
mod sql;
mod target;
mod transaction;

pub use export::{MySqlExportPlan, mysql_export_plan};
pub use sql::{
    build_bulk_delete_sql, build_explain_analyze_sql, build_explain_sql, build_update_sql,
};

use lexer::TokenKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MySqlStatementKind {
    Select,
    Table,
    Show,
    Describe,
    Insert,
    Replace,
    Update { has_where: bool },
    Delete { has_where: bool },
    CreateTable { temporary: bool },
    AlterTable,
    RenameTable,
    DropTable { temporary: bool },
    TruncateTable,
    CreateView,
    AlterView,
    DropView,
    CreateIndex,
    DropIndex,
    Begin,
    StartTransaction,
    Commit,
    Rollback,
    Savepoint,
    RollbackToSavepoint,
    ReleaseSavepoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MySqlStatement {
    sql: String,
    kind: MySqlStatementKind,
    on_duplicate_key_update: bool,
    target: Option<String>,
    target_database: Option<String>,
}

impl MySqlStatement {
    pub fn sql(&self) -> &str {
        &self.sql
    }

    pub fn kind(&self) -> &MySqlStatementKind {
        &self.kind
    }

    pub fn has_on_duplicate_key_update(&self) -> bool {
        self.on_duplicate_key_update
    }

    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct MySqlLexError(pub String);

pub fn split_mysql_statements(sql: &str) -> Result<Vec<String>, MySqlLexError> {
    lexer::split_mysql_statements(sql)
}

pub fn classify_mysql_statement(sql: &str) -> Result<MySqlStatement, MySqlLexError> {
    let tokens = lexer::lex_mysql_statement(sql)?;
    if tokens.is_empty() {
        return Err(MySqlLexError("empty MySQL statement".to_string()));
    }
    if tokens.iter().any(|token| {
        matches!(
            &token.kind,
            TokenKind::Word(word) if word == "UNSUPPORTED_VERSION_COMMENT"
        )
    }) {
        return Err(MySqlLexError(
            "executable MySQL version comment contains another statement".to_string(),
        ));
    }
    let (kind, target, target_database) = classifier::kind_and_target(&tokens)?;
    Ok(MySqlStatement {
        sql: sql.to_string(),
        on_duplicate_key_update: matches!(kind, MySqlStatementKind::Insert)
            && classifier::has_on_duplicate_key_update(&tokens),
        kind,
        target,
        target_database,
    })
}

fn has_top_level_into_clause(sql: &str) -> Result<bool, MySqlLexError> {
    side_effect::has_top_level_into_clause(sql)
}

pub fn has_top_level_user_variable_into_clause(sql: &str) -> Result<bool, MySqlLexError> {
    side_effect::has_top_level_user_variable_into_clause(sql)
}

pub fn has_mysql_read_only_side_effect(sql: &str) -> Result<bool, MySqlLexError> {
    side_effect::has_mysql_read_only_side_effect(sql)
}

pub fn mysql_statement_reads_session_diagnostics(sql: &str) -> Result<bool, MySqlLexError> {
    side_effect::mysql_statement_reads_session_diagnostics(sql)
}

fn target_is_selected_database_with_lower_case_table_names(
    statement: &MySqlStatement,
    selected_database: Option<&str>,
    lower_case_table_names: u8,
) -> bool {
    target::target_is_selected_database_with_lower_case_table_names(
        statement,
        selected_database,
        lower_case_table_names,
    )
}

pub fn statement_contains_unsupported_mysql_control(sql: &str) -> bool {
    side_effect::statement_contains_unsupported_mysql_control(sql)
}

pub fn mysql_statement_is_schema_modifying(kind: &MySqlStatementKind) -> bool {
    matches!(
        kind,
        MySqlStatementKind::CreateTable { .. }
            | MySqlStatementKind::AlterTable
            | MySqlStatementKind::RenameTable
            | MySqlStatementKind::DropTable { .. }
            | MySqlStatementKind::TruncateTable
            | MySqlStatementKind::CreateView
            | MySqlStatementKind::AlterView
            | MySqlStatementKind::DropView
            | MySqlStatementKind::CreateIndex
            | MySqlStatementKind::DropIndex
    )
}

pub fn mysql_statement_is_data_modifying(kind: &MySqlStatementKind) -> bool {
    matches!(
        kind,
        MySqlStatementKind::Insert
            | MySqlStatementKind::Replace
            | MySqlStatementKind::Update { .. }
            | MySqlStatementKind::Delete { .. }
    )
}

pub fn mysql_statement_is_persistent_schema_change(kind: &MySqlStatementKind) -> bool {
    mysql_statement_is_schema_modifying(kind)
        && !matches!(
            kind,
            MySqlStatementKind::CreateTable { temporary: true }
                | MySqlStatementKind::DropTable { temporary: true }
        )
}

pub fn classify_mysql_multi_statement(
    sql: &str,
    selected_database: Option<&str>,
) -> Result<Vec<MySqlStatement>, String> {
    classify_mysql_multi_statement_with_lower_case_table_names(sql, selected_database, 0)
}

pub fn classify_mysql_multi_statement_with_lower_case_table_names(
    sql: &str,
    selected_database: Option<&str>,
    lower_case_table_names: u8,
) -> Result<Vec<MySqlStatement>, String> {
    if statement_contains_unsupported_mysql_control(sql) {
        return Err("unsupported MySQL session or table-lock statement".to_string());
    }
    let statements = match split_mysql_statements(sql) {
        Ok(statements) if !statements.is_empty() => statements,
        Ok(_) => return Err("Empty MySQL input".to_string()),
        Err(error) => return Err(error.to_string()),
    };

    let mut classified = Vec::with_capacity(statements.len());
    for statement_sql in statements {
        let statement =
            classify_mysql_statement(&statement_sql).map_err(|error| error.to_string())?;
        classified.push(statement);
    }

    validate_mysql_statements_with_lower_case_table_names(
        &classified,
        selected_database,
        lower_case_table_names,
    )?;
    Ok(classified)
}

pub fn validate_mysql_statements(
    statements: &[MySqlStatement],
    selected_database: Option<&str>,
) -> Result<(), String> {
    validate_mysql_statements_with_lower_case_table_names(statements, selected_database, 0)
}

pub fn validate_mysql_statements_with_lower_case_table_names(
    statements: &[MySqlStatement],
    selected_database: Option<&str>,
    lower_case_table_names: u8,
) -> Result<(), String> {
    if statements.is_empty() {
        return Err("Empty MySQL input".to_string());
    }

    for statement in statements {
        if statement_contains_unsupported_mysql_control(&statement.sql) {
            return Err("unsupported MySQL session or table-lock statement".to_string());
        }
        let has_unsupported_into_clause = has_top_level_into_clause(&statement.sql).unwrap_or(true)
            && !(matches!(statement.kind, MySqlStatementKind::Select)
                && has_top_level_user_variable_into_clause(&statement.sql).unwrap_or(false));
        if matches!(
            statement.kind,
            MySqlStatementKind::Select | MySqlStatementKind::Table
        ) && has_unsupported_into_clause
        {
            return Err("MySQL SELECT INTO clauses are not supported".to_string());
        }
        if (mysql_statement_is_schema_modifying(&statement.kind)
            || mysql_statement_is_data_modifying(&statement.kind))
            && !target_is_selected_database_with_lower_case_table_names(
                statement,
                selected_database,
                lower_case_table_names,
            )
        {
            return Err("MySQL target must be in the selected database".to_string());
        }
    }

    validate_mysql_submission_state(statements, selected_database, lower_case_table_names)
}

fn mysql_target_key(
    statement: &MySqlStatement,
    selected_database: Option<&str>,
    lower_case_table_names: u8,
) -> Option<String> {
    let database = statement
        .target_database
        .as_deref()
        .or(selected_database)
        .unwrap_or_default();
    let database = match lower_case_table_names {
        0 => database.to_string(),
        1 | 2 => database.to_lowercase(),
        _ => return None,
    };
    let target = statement.target.as_deref()?;
    let target = match lower_case_table_names {
        0 => target.to_string(),
        1 | 2 => target.to_lowercase(),
        _ => return None,
    };
    Some(format!("{database}:{target}"))
}

fn mysql_names_equal(left: &str, right: &str) -> bool {
    left.to_uppercase() == right.to_uppercase()
}

fn validate_mysql_submission_state(
    statements: &[MySqlStatement],
    selected_database: Option<&str>,
    lower_case_table_names: u8,
) -> Result<(), String> {
    let mut transaction_open = false;
    let mut savepoints = Vec::<String>::new();
    let mut temporary_tables = Vec::<String>::new();

    for statement in statements {
        match &statement.kind {
            MySqlStatementKind::Begin | MySqlStatementKind::StartTransaction => {
                if transaction_open {
                    return Err("nested MySQL transactions are not supported".to_string());
                }
                transaction_open = true;
                savepoints.clear();
            }
            MySqlStatementKind::Commit | MySqlStatementKind::Rollback => {
                transaction_open = false;
                savepoints.clear();
            }
            MySqlStatementKind::Savepoint => {
                if !transaction_open {
                    return Err("MySQL SAVEPOINT requires an explicit transaction".to_string());
                }
                let name = statement
                    .target
                    .as_deref()
                    .ok_or_else(|| "MySQL SAVEPOINT name is ambiguous".to_string())?;
                savepoints.retain(|current| !mysql_names_equal(current, name));
                savepoints.push(name.to_string());
            }
            MySqlStatementKind::RollbackToSavepoint => {
                if !transaction_open {
                    return Err(
                        "MySQL ROLLBACK TO SAVEPOINT requires an explicit transaction".to_string(),
                    );
                }
                let name = statement
                    .target
                    .as_deref()
                    .ok_or_else(|| "MySQL SAVEPOINT name is ambiguous".to_string())?;
                let Some(index) = savepoints
                    .iter()
                    .position(|current| mysql_names_equal(current, name))
                else {
                    return Err("MySQL ROLLBACK TO SAVEPOINT name is unknown".to_string());
                };
                savepoints.truncate(index + 1);
            }
            MySqlStatementKind::ReleaseSavepoint => {
                if !transaction_open {
                    return Err(
                        "MySQL RELEASE SAVEPOINT requires an explicit transaction".to_string()
                    );
                }
                let name = statement
                    .target
                    .as_deref()
                    .ok_or_else(|| "MySQL SAVEPOINT name is ambiguous".to_string())?;
                let Some(index) = savepoints
                    .iter()
                    .position(|current| mysql_names_equal(current, name))
                else {
                    return Err("MySQL RELEASE SAVEPOINT name is unknown".to_string());
                };
                savepoints.remove(index);
            }
            MySqlStatementKind::CreateTable { temporary: true } => {
                let key = mysql_target_key(statement, selected_database, lower_case_table_names)
                    .ok_or_else(|| "MySQL temporary table target is ambiguous".to_string())?;
                if temporary_tables.iter().any(|current| current == &key) {
                    return Err("MySQL temporary table is created more than once".to_string());
                }
                temporary_tables.push(key);
            }
            MySqlStatementKind::DropTable { temporary: true } => {
                let key = mysql_target_key(statement, selected_database, lower_case_table_names)
                    .ok_or_else(|| "MySQL temporary table target is ambiguous".to_string())?;
                let Some(index) = temporary_tables.iter().position(|current| current == &key)
                else {
                    return Err(
                        "MySQL temporary tables must be created and dropped in one submission"
                            .to_string(),
                    );
                };
                temporary_tables.remove(index);
            }
            kind if transaction_open && mysql_statement_is_persistent_schema_change(kind) => {
                return Err(
                    "MySQL persistent DDL causes an implicit commit and cannot be rolled back with the surrounding transaction"
                        .to_string(),
                );
            }
            _ => {}
        }
    }

    if transaction_open {
        return Err("MySQL explicit transaction must finish in one submission".to_string());
    }
    Ok(())
}

pub fn mysql_explain_rejection_message(sql: &str) -> Option<&'static str> {
    let Ok(statements) = split_mysql_statements(sql) else {
        return Some("MySQL EXPLAIN requires one valid statement");
    };
    if statements.len() != 1 {
        return Some("MySQL EXPLAIN does not support multiple statements");
    }
    if statement_contains_unsupported_mysql_control(sql) {
        return Some("MySQL EXPLAIN does not support MySQL client commands");
    }

    let Ok(statement) = classify_mysql_statement(&statements[0]) else {
        return Some(
            "MySQL EXPLAIN supports SELECT, TABLE, INSERT, REPLACE, UPDATE, or DELETE statements",
        );
    };
    (!matches!(
        statement.kind,
        MySqlStatementKind::Select
            | MySqlStatementKind::Table
            | MySqlStatementKind::Insert
            | MySqlStatementKind::Replace
            | MySqlStatementKind::Update { .. }
            | MySqlStatementKind::Delete { .. }
    ))
    .then_some(
        "MySQL EXPLAIN supports SELECT, TABLE, INSERT, REPLACE, UPDATE, or DELETE statements",
    )
}

pub fn mysql_tree_explain_query_kind(sql: &str) -> Option<bool> {
    let trimmed = sql.trim();
    let (is_analyze, target) = strip_mysql_tree_explain_prefix(trimmed)?;
    let target = target.trim();

    let valid = if is_analyze {
        !statement_contains_unsupported_mysql_control(target)
            && split_mysql_statements(target).is_ok_and(|statements| {
                statements.len() == 1
                    && classify_mysql_statement(&statements[0]).is_ok_and(|statement| {
                        matches!(
                            statement.kind,
                            MySqlStatementKind::Select | MySqlStatementKind::Table
                        ) && !has_mysql_read_only_side_effect(target).unwrap_or(true)
                    })
            })
    } else {
        mysql_explain_rejection_message(target).is_none()
    };
    valid.then_some(is_analyze)
}

fn strip_mysql_tree_explain_prefix(text: &str) -> Option<(bool, &str)> {
    let mut index = 0;
    consume_mysql_keyword(text, &mut index, "EXPLAIN")?;
    consume_required_ascii_whitespace(text, &mut index)?;

    let is_analyze = if consume_mysql_keyword(text, &mut index, "ANALYZE").is_some() {
        consume_required_ascii_whitespace(text, &mut index)?;
        true
    } else {
        false
    };

    consume_mysql_keyword(text, &mut index, "FORMAT")?;
    skip_ascii_whitespace(text, &mut index);
    if text.as_bytes().get(index) != Some(&b'=') {
        return None;
    }
    index += 1;
    skip_ascii_whitespace(text, &mut index);
    consume_mysql_keyword(text, &mut index, "TREE")?;

    let target = text.get(index..)?;
    if target.is_empty() || !target.as_bytes()[0].is_ascii_whitespace() {
        return None;
    }
    Some((is_analyze, target))
}

fn consume_mysql_keyword(text: &str, index: &mut usize, keyword: &str) -> Option<()> {
    let candidate = text.get(*index..(*index).saturating_add(keyword.len()))?;
    if !candidate.eq_ignore_ascii_case(keyword) {
        return None;
    }
    if text
        .as_bytes()
        .get(index.saturating_add(keyword.len()))
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
    {
        return None;
    }
    *index += keyword.len();
    Some(())
}

fn skip_ascii_whitespace(text: &str, index: &mut usize) {
    while text
        .as_bytes()
        .get(*index)
        .is_some_and(u8::is_ascii_whitespace)
    {
        *index += 1;
    }
}

fn consume_required_ascii_whitespace(text: &str, index: &mut usize) -> Option<()> {
    let start = *index;
    skip_ascii_whitespace(text, index);
    (*index > start).then_some(())
}

#[cfg(test)]
#[path = "mysql_sql_tests.rs"]
mod tests;
