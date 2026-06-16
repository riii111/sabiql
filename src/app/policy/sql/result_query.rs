use super::statement_classifier::{self, StatementKind};

pub fn is_rerunnable_select(sql: &str) -> bool {
    matches!(statement_classifier::classify(sql), StatementKind::Select)
        && !statement_classifier::has_executed_data_modifying_cte(sql)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::select("SELECT * FROM users")]
    #[case::show("SHOW search_path")]
    fn allows_read_only_result_queries(#[case] sql: &str) {
        assert!(is_rerunnable_select(sql));
    }

    #[rstest]
    #[case::insert_returning("INSERT INTO users(name) VALUES ('a') RETURNING id")]
    #[case::update_returning("UPDATE users SET name = 'b' WHERE id = 1 RETURNING id")]
    #[case::delete_returning("DELETE FROM users WHERE id = 1 RETURNING id")]
    #[case::cte_update_returning(
        "WITH changed AS (SELECT 1) UPDATE users SET name = 'b' RETURNING id"
    )]
    #[case::select_from_update_cte(
        "WITH moved AS (UPDATE users SET name = 'a' RETURNING *) SELECT * FROM moved"
    )]
    #[case::select_from_insert_cte(
        "WITH inserted AS (INSERT INTO users(name) VALUES ('a') RETURNING *) SELECT * FROM inserted"
    )]
    #[case::select_from_delete_cte(
        "WITH deleted AS (DELETE FROM users WHERE id = 1 RETURNING *) SELECT * FROM deleted"
    )]
    #[case::explain_analyze_update_cte(
        "EXPLAIN ANALYZE WITH moved AS (UPDATE users SET name = 'a' RETURNING *) SELECT * FROM moved"
    )]
    #[case::multi_statement("SELECT 1; DELETE FROM users RETURNING id")]
    #[case::explain_analyze_update("EXPLAIN ANALYZE UPDATE users SET name = 'b' RETURNING id")]
    fn blocks_queries_that_could_mutate_on_rerun(#[case] sql: &str) {
        assert!(!is_rerunnable_select(sql));
    }

    #[test]
    fn allows_plain_explain_with_data_modifying_cte() {
        let sql =
            "EXPLAIN WITH moved AS (UPDATE users SET name = 'a' RETURNING *) SELECT * FROM moved";

        assert!(is_rerunnable_select(sql));
    }
}
