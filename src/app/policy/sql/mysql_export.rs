use super::mysql_statement::{
    MysqlStatementKind, classify_mysql_statement, split_mysql_statements,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MysqlExportPlan {
    CountRows,
    UseResultRowCount,
}

pub fn mysql_export_plan(query: &str) -> Option<MysqlExportPlan> {
    let statements = split_mysql_statements(query).ok()?;
    let [statement] = statements.as_slice() else {
        return None;
    };
    let statement = classify_mysql_statement(statement).ok()?;

    match statement.kind {
        MysqlStatementKind::Select => Some(MysqlExportPlan::CountRows),
        MysqlStatementKind::Table | MysqlStatementKind::Show | MysqlStatementKind::Describe => {
            Some(MysqlExportPlan::UseResultRowCount)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::select("SELECT id FROM users", MysqlExportPlan::CountRows)]
    #[case::table("TABLE users", MysqlExportPlan::UseResultRowCount)]
    #[case::show("SHOW TABLES", MysqlExportPlan::UseResultRowCount)]
    #[case::describe("DESCRIBE users", MysqlExportPlan::UseResultRowCount)]
    fn plans_supported_mysql_export_queries(
        #[case] query: &str,
        #[case] expected: MysqlExportPlan,
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
