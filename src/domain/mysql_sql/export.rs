use super::{MySqlStatementKind, classify_mysql_multi_statement, has_mysql_read_only_side_effect};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MySqlExportPlan {
    CountRows { statement: String },
    UseResultRowCount { statement: String },
}

pub fn mysql_export_plan(query: &str) -> Option<MySqlExportPlan> {
    let statements = classify_mysql_multi_statement(query, None).ok()?;
    let [statement] = statements.as_slice() else {
        return None;
    };

    if !matches!(
        statement.kind,
        MySqlStatementKind::Select
            | MySqlStatementKind::Table
            | MySqlStatementKind::Show
            | MySqlStatementKind::Describe
    ) || has_mysql_read_only_side_effect(&statement.sql).unwrap_or(true)
    {
        return None;
    }

    match statement.kind {
        MySqlStatementKind::Select => Some(MySqlExportPlan::CountRows {
            statement: statement.sql.clone(),
        }),
        MySqlStatementKind::Table | MySqlStatementKind::Show | MySqlStatementKind::Describe => {
            Some(MySqlExportPlan::UseResultRowCount {
                statement: statement.sql.clone(),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::select("SELECT id FROM users", MySqlExportPlan::CountRows { statement: "SELECT id FROM users".to_string() })]
    #[case::table("TABLE users", MySqlExportPlan::UseResultRowCount { statement: "TABLE users".to_string() })]
    #[case::show("SHOW TABLES", MySqlExportPlan::UseResultRowCount { statement: "SHOW TABLES".to_string() })]
    #[case::describe("DESCRIBE users", MySqlExportPlan::UseResultRowCount { statement: "DESCRIBE users".to_string() })]
    fn plans_supported_mysql_export_queries(
        #[case] query: &str,
        #[case] expected: MySqlExportPlan,
    ) {
        assert_eq!(mysql_export_plan(query), Some(expected));
    }

    #[rstest]
    #[case::insert("INSERT INTO users VALUES (1)")]
    #[case::multiple_statements("SELECT 1; SELECT 2")]
    #[case::unknown("GRANT SELECT ON users TO 'user'")]
    #[case::side_effect("SELECT GET_LOCK('sabiql', 0)")]
    #[case::client_control("SET sql_mode = 'STRICT_TRANS_TABLES'")]
    fn rejects_queries_without_a_safe_mysql_export_plan(#[case] query: &str) {
        assert_eq!(mysql_export_plan(query), None);
    }

    #[test]
    fn plan_uses_the_single_normalized_statement_after_a_trailing_comment() {
        assert_eq!(
            mysql_export_plan("SELECT id FROM users; -- trailing comment"),
            Some(MySqlExportPlan::CountRows {
                statement: "SELECT id FROM users".to_string(),
            })
        );
    }
}
