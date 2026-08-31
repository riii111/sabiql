use std::borrow::Cow;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::app::ports::outbound::DbOperationError;
use crate::domain::sqlite_sql::{
    SqliteTransactionPolicy, sqlite_statement_classification,
    sqlite_transaction_policy_for_classifications,
};

use super::lexer::{contains_keyword, dml_keyword, first_keyword, try_split_sqlite_statements};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SqliteWrapMode {
    None,
    BeginCommit,
}

#[derive(Debug)]
pub(in crate::adapters::sqlite) struct SqliteStatementPlan<'a> {
    query: &'a str,
    statements: Vec<&'a str>,
    wrap_mode: SqliteWrapMode,
}

impl<'a> SqliteStatementPlan<'a> {
    pub(in crate::adapters::sqlite::sqlite3) fn query(&self) -> &'a str {
        self.query
    }

    pub(in crate::adapters::sqlite) fn statements(&self) -> &[&'a str] {
        &self.statements
    }

    pub(in crate::adapters::sqlite::sqlite3) fn is_dml(&self, index: usize) -> bool {
        is_dml_statement(self.statements[index])
    }

    fn wrap_mode(&self) -> SqliteWrapMode {
        self.wrap_mode
    }
}

pub(in crate::adapters::sqlite) fn sqlite_statement_plan(
    query: &str,
) -> Result<SqliteStatementPlan<'_>, DbOperationError> {
    let statements = try_split_sqlite_statements(query)?;
    let classes: Vec<_> = statements
        .iter()
        .map(|statement| sqlite_statement_classification(statement))
        .collect();
    let wrap_mode = if sqlite_transaction_policy_for_classifications(statements.len(), &classes)
        == SqliteTransactionPolicy::AutoWrap
    {
        SqliteWrapMode::BeginCommit
    } else {
        SqliteWrapMode::None
    };
    Ok(SqliteStatementPlan {
        query,
        statements,
        wrap_mode,
    })
}

fn sqlite_transaction_block(query: &str) -> String {
    let trimmed = query.trim_end().trim_end_matches(';').trim_end();
    format!("BEGIN;\n{trimmed}\n;\nCOMMIT")
}

fn sqlite_execution_query_for_plan<'query>(plan: &SqliteStatementPlan<'query>) -> Cow<'query, str> {
    match plan.wrap_mode() {
        SqliteWrapMode::BeginCommit => Cow::Owned(sqlite_transaction_block(plan.query())),
        SqliteWrapMode::None => Cow::Borrowed(plan.query()),
    }
}

pub(in crate::adapters::sqlite) fn sqlite_probe_marker() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "__sabiql_sqlite_probe_{}_{}_{}",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

pub(super) fn sqlite_probe_columns(marker: &str) -> (String, String) {
    (format!("{marker}_stmt"), format!("{marker}_changes"))
}

fn sqlite_changes_probe(marker: &str, index: usize) -> String {
    let (stmt_col, changes_col) = sqlite_probe_columns(marker);
    format!("SELECT {index} AS \"{stmt_col}\", changes() AS \"{changes_col}\"")
}

pub(super) fn sqlite_result_probe_columns(marker: &str) -> (String, String) {
    (
        format!("{marker}_result_stmt"),
        format!("{marker}_result_marker"),
    )
}

fn sqlite_result_probe(marker: &str, index: usize) -> String {
    let (stmt_col, marker_col) = sqlite_result_probe_columns(marker);
    format!("SELECT {index} AS \"{stmt_col}\", '{marker}' AS \"{marker_col}\"")
}

fn sqlite_empty_result_frame(statement: &str, marker: &str) -> String {
    let sentinel = sqlite_empty_result_sentinel(marker);
    format!(
        "SELECT _s.* FROM (SELECT 1) AS _p LEFT JOIN (SELECT _q.*, 1 AS \"{sentinel}\" FROM ({statement}) AS _q) AS _s ON true"
    )
}

pub(in crate::adapters::sqlite) fn sqlite_empty_result_sentinel(marker: &str) -> String {
    format!("{marker}_empty")
}

pub(in crate::adapters::sqlite) fn sqlite_adhoc_execution_query_for_plan(
    plan: &SqliteStatementPlan<'_>,
    marker: &str,
) -> String {
    let statements = plan.statements();
    if statements.is_empty() {
        return plan.query().to_string();
    }

    let wrap_mode = plan.wrap_mode();
    let mut parts = Vec::with_capacity(statements.len() * 2 + 2);
    if matches!(wrap_mode, SqliteWrapMode::BeginCommit) {
        parts.push("BEGIN".to_string());
    }
    for (index, statement) in statements.iter().enumerate() {
        if first_keyword(statement).eq_ignore_ascii_case("SELECT")
            || (first_keyword(statement).eq_ignore_ascii_case("WITH")
                && !is_dml_statement(statement))
        {
            parts.push(sqlite_empty_result_frame(statement, marker));
        } else {
            parts.push((*statement).to_string());
        }
        if plan.is_dml(index) {
            parts.push(sqlite_changes_probe(marker, index));
        }
        if statement_emits_result_set(statement) {
            parts.push(sqlite_result_probe(marker, index));
        }
    }
    if matches!(wrap_mode, SqliteWrapMode::BeginCommit) {
        parts.push("COMMIT".to_string());
    }
    parts.join("\n;\n")
}

pub(in crate::adapters::sqlite) fn append_changes_query_for_plan(
    plan: &SqliteStatementPlan<'_>,
) -> String {
    let body = sqlite_execution_query_for_plan(plan).trim_end().to_string();
    // The standalone separator also terminates a trailing line comment before
    // appending the changes() probe.
    format!("{body}\n;\nSELECT changes() AS affected_rows;")
}

fn is_dml_statement(statement: &str) -> bool {
    dml_keyword(statement).is_some()
}

fn statement_emits_result_set(statement: &str) -> bool {
    let keyword = first_keyword(statement);
    if keyword.eq_ignore_ascii_case("SELECT")
        || keyword.eq_ignore_ascii_case("PRAGMA")
        || keyword.eq_ignore_ascii_case("EXPLAIN")
        || keyword.eq_ignore_ascii_case("VALUES")
    {
        return true;
    }
    if is_dml_statement(statement) {
        return contains_keyword(statement, "RETURNING");
    }
    keyword.eq_ignore_ascii_case("WITH")
}

#[cfg(test)]
mod tests {
    use crate::domain::sqlite_sql::SqliteStatementClassification;

    use super::*;
    use rstest::rstest;

    fn sqlite_wrap_mode(query: &str) -> Result<SqliteWrapMode, DbOperationError> {
        Ok(sqlite_statement_plan(query)?.wrap_mode())
    }

    fn sqlite_adhoc_execution_query(query: &str, marker: &str) -> Result<String, DbOperationError> {
        let plan = sqlite_statement_plan(query)?;
        Ok(sqlite_adhoc_execution_query_for_plan(&plan, marker))
    }

    fn append_changes_query(query: &str) -> Result<String, DbOperationError> {
        let plan = sqlite_statement_plan(query)?;
        Ok(append_changes_query_for_plan(&plan))
    }

    mod execution_probes {
        use super::*;

        #[test]
        fn do_not_insert_probes_when_trigger_references_new_end() {
            let trigger = "\
CREATE TRIGGER sync_end AFTER UPDATE ON events BEGIN
    UPDATE counters SET end_value = new.end WHERE id = new.id;
    INSERT INTO audit(event_id, end_value) VALUES (new.id, new.end);
END";
            let marker = "probe_marker";

            let execution_query = sqlite_adhoc_execution_query(trigger, marker).unwrap();

            assert!(!execution_query.contains(marker));
            assert_eq!(execution_query, trigger);
        }

        #[test]
        fn do_not_insert_probes_inside_create_trigger() {
            let trigger = "\
CREATE TRIGGER agent_messages_fts_ai AFTER INSERT ON agent_messages BEGIN
    INSERT INTO agent_messages_fts(rowid, role, content)
    VALUES (new.id, new.role, new.content);
END";
            let marker = "probe_marker";

            let execution_query = sqlite_adhoc_execution_query(trigger, marker).unwrap();

            assert!(!execution_query.contains(marker));
            assert_eq!(execution_query, trigger);
        }
    }

    mod changes_query {
        use super::*;

        #[test]
        fn wraps_multi_statement_write_without_explicit_transaction() {
            let query = "INSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2);";

            let wrapped = append_changes_query(query).unwrap();

            assert_eq!(
                wrapped,
                "BEGIN;\nINSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2)\n;\nCOMMIT\n;\nSELECT changes() AS affected_rows;"
            );
        }

        #[test]
        fn wraps_multi_statement_replace_without_explicit_transaction() {
            let query = "REPLACE INTO users(id) VALUES (1); SELECT * FROM missing";

            let wrapped = append_changes_query(query).unwrap();

            assert_eq!(
                wrapped,
                "BEGIN;\nREPLACE INTO users(id) VALUES (1); SELECT * FROM missing\n;\nCOMMIT\n;\nSELECT changes() AS affected_rows;"
            );
        }

        #[test]
        fn wraps_multi_statement_with_write_without_explicit_transaction() {
            let query = "WITH payload(id) AS (VALUES (1)) INSERT INTO users(id) SELECT id FROM payload; SELECT * FROM missing";

            let wrapped = append_changes_query(query).unwrap();

            assert_eq!(
                wrapped,
                "BEGIN;\nWITH payload(id) AS (VALUES (1)) INSERT INTO users(id) SELECT id FROM payload; SELECT * FROM missing\n;\nCOMMIT\n;\nSELECT changes() AS affected_rows;"
            );
        }

        #[test]
        fn keeps_transaction_incompatible_statement_outside_auto_transaction() {
            let query = "INSERT INTO users(id) VALUES (1); VACUUM";

            let wrapped = append_changes_query(query).unwrap();

            assert_eq!(
                wrapped,
                "INSERT INTO users(id) VALUES (1); VACUUM\n;\nSELECT changes() AS affected_rows;"
            );
        }

        #[test]
        fn keeps_explicit_begin_commit_transaction_control() {
            let query = "BEGIN; INSERT INTO users(id) VALUES (1); COMMIT";

            let wrapped = append_changes_query(query).unwrap();

            assert_eq!(
                wrapped,
                "BEGIN; INSERT INTO users(id) VALUES (1); COMMIT\n;\nSELECT changes() AS affected_rows;"
            );
        }

        #[test]
        fn keeps_explicit_begin_end_transaction_control() {
            let query = "BEGIN; INSERT INTO users(id) VALUES (1); END";

            let wrapped = append_changes_query(query).unwrap();

            assert_eq!(
                wrapped,
                "BEGIN; INSERT INTO users(id) VALUES (1); END\n;\nSELECT changes() AS affected_rows;"
            );
        }
    }

    mod transaction_wrap_mode {
        use super::*;

        #[rstest]
        #[case::multi_dml("INSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2)")]
        #[case::trailing_comment_only("INSERT INTO users(id) VALUES (1); -- trailing comment")]
        #[case::read_only_pragma_with_writes(
            "PRAGMA journal_mode; INSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2)"
        )]
        #[case::ddl_and_dml(
            "CREATE TABLE users(id INTEGER PRIMARY KEY); INSERT INTO users(id) VALUES (1)"
        )]
        fn compatible_write_batches_use_auto_transaction(#[case] query: &str) {
            assert_eq!(
                sqlite_wrap_mode(query).unwrap(),
                SqliteWrapMode::BeginCommit
            );
        }

        #[rstest]
        #[case::explicit_transaction("BEGIN; INSERT INTO users(id) VALUES (1); COMMIT")]
        #[case::top_level_savepoint(
            "SAVEPOINT user_sp; INSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2)"
        )]
        #[case::mid_batch_savepoint(
            "INSERT INTO users(id) VALUES (1); SAVEPOINT sp; INSERT INTO users(id) VALUES (2)"
        )]
        #[case::vacuum("INSERT INTO users(id) VALUES (1); VACUUM")]
        #[case::journal_mode_change(
            "PRAGMA journal_mode = WAL; CREATE TABLE users(id INTEGER PRIMARY KEY)"
        )]
        #[case::quoted_foreign_keys_change(
            "/* setup */ PRAGMA [foreign_keys](OFF); CREATE TABLE users(id INTEGER PRIMARY KEY)"
        )]
        fn user_managed_or_incompatible_batches_skip_auto_transaction(#[case] query: &str) {
            assert_eq!(sqlite_wrap_mode(query).unwrap(), SqliteWrapMode::None);
        }
    }

    mod statement_classification {
        use super::*;

        #[test]
        fn distinguishes_journal_mode_query_from_change() {
            assert_eq!(
                sqlite_statement_classification("PRAGMA journal_mode"),
                SqliteStatementClassification::ReadOnly
            );
            assert_eq!(
                sqlite_statement_classification("PRAGMA main.journal_mode = WAL"),
                SqliteStatementClassification::TransactionIncompatible
            );
            assert_eq!(
                sqlite_statement_classification("PRAGMA journal_mode(WAL)"),
                SqliteStatementClassification::TransactionIncompatible
            );
            assert_eq!(
                sqlite_statement_classification("PRAGMA foreign_keys = OFF"),
                SqliteStatementClassification::TransactionIncompatible
            );
            assert_eq!(
                sqlite_statement_classification("PRAGMA foreign_keys"),
                SqliteStatementClassification::ReadOnly
            );
            assert_eq!(
                sqlite_statement_classification("PRAGMA \"foreign_keys\" = OFF"),
                SqliteStatementClassification::TransactionIncompatible
            );
            assert_eq!(
                sqlite_statement_classification("/* setup */ PRAGMA [foreign_keys](OFF)"),
                SqliteStatementClassification::TransactionIncompatible
            );
        }

        #[test]
        fn classifies_vacuum_and_writes_for_auto_transaction_policy() {
            assert_eq!(
                sqlite_statement_classification("VACUUM INTO 'backup.db'"),
                SqliteStatementClassification::TransactionIncompatible
            );
            assert_eq!(
                sqlite_statement_classification("CREATE TABLE users(id INTEGER PRIMARY KEY)"),
                SqliteStatementClassification::TransactionalWrite
            );
            assert_eq!(
                sqlite_statement_classification("BEGIN"),
                SqliteStatementClassification::TransactionControl
            );
        }
    }
}
