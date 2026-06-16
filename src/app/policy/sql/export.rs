use super::statement_classifier::{self, StatementKind};

pub fn can_rerun_for_csv_export(sql: &str) -> bool {
    matches!(statement_classifier::classify(sql), StatementKind::Select)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::select("SELECT * FROM users")]
    #[case::show("SHOW search_path")]
    fn allows_read_only_result_queries(#[case] sql: &str) {
        assert!(can_rerun_for_csv_export(sql));
    }

    #[rstest]
    #[case::insert_returning("INSERT INTO users(name) VALUES ('a') RETURNING id")]
    #[case::update_returning("UPDATE users SET name = 'b' WHERE id = 1 RETURNING id")]
    #[case::delete_returning("DELETE FROM users WHERE id = 1 RETURNING id")]
    #[case::cte_update_returning(
        "WITH changed AS (SELECT 1) UPDATE users SET name = 'b' RETURNING id"
    )]
    #[case::multi_statement("SELECT 1; DELETE FROM users RETURNING id")]
    #[case::explain_analyze_update("EXPLAIN ANALYZE UPDATE users SET name = 'b' RETURNING id")]
    fn blocks_queries_that_could_mutate_on_rerun(#[case] sql: &str) {
        assert!(!can_rerun_for_csv_export(sql));
    }
}
