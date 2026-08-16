use super::mysql_statement::{
    MySqlStatementKind, classify_mysql_statement, split_mysql_statements,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MySqlExportPlan {
    CountRows,
    UseResultRowCount,
}

pub fn mysql_export_plan(query: &str) -> Option<MySqlExportPlan> {
    let statements = split_mysql_statements(query).ok()?;
    let [statement] = statements.as_slice() else {
        return None;
    };
    let statement = classify_mysql_statement(statement).ok()?;

    match statement.kind {
        MySqlStatementKind::Select => Some(MySqlExportPlan::CountRows),
        MySqlStatementKind::Table | MySqlStatementKind::Show | MySqlStatementKind::Describe => {
            Some(MySqlExportPlan::UseResultRowCount)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::select("SELECT id FROM users", MySqlExportPlan::CountRows)]
    #[case::table("TABLE users", MySqlExportPlan::UseResultRowCount)]
    #[case::show("SHOW TABLES", MySqlExportPlan::UseResultRowCount)]
    #[case::describe("DESCRIBE users", MySqlExportPlan::UseResultRowCount)]
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
    fn rejects_queries_without_a_safe_mysql_export_plan(#[case] query: &str) {
        assert_eq!(mysql_export_plan(query), None);
    }
}
