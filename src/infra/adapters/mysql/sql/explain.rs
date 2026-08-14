use crate::app::policy::sql::mysql_statement::{
    mysql_explain_rejection_message, mysql_tree_explain_query_kind,
};

pub(super) fn build_explain_sql(query: &str) -> Option<String> {
    if mysql_explain_rejection_message(query).is_some() {
        return None;
    }
    Some(format!("EXPLAIN FORMAT=TREE {query}"))
}

pub(super) fn build_explain_analyze_sql(query: &str) -> Option<String> {
    mysql_tree_explain_query_kind(&format!("EXPLAIN ANALYZE FORMAT=TREE {query}"))?;
    Some(format!("EXPLAIN ANALYZE FORMAT=TREE {query}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_tree_explain_for_supported_queries() {
        for query in [
            "SELECT * FROM users",
            "TABLE users",
            "INSERT INTO users VALUES (1)",
            "REPLACE INTO users VALUES (1)",
            "REPLACE users VALUES (1)",
            "REPLACE LOW_PRIORITY INTO users VALUES (1)",
            "REPLACE DELAYED users VALUES (1)",
            "UPDATE users SET name = 'Ada' WHERE id = 1",
            "DELETE FROM users WHERE id = 1",
        ] {
            assert_eq!(
                build_explain_sql(query),
                Some(format!("EXPLAIN FORMAT=TREE {query}")),
                "{query}"
            );
        }
    }

    #[test]
    fn rejects_tree_explain_for_unsupported_input() {
        for query in [
            "CREATE TABLE users(id INT)",
            "DROP TABLE users",
            "\\C /tmp/other.sock",
            "SELECT 1; SELECT 2",
        ] {
            assert_eq!(build_explain_sql(query), None, "{query}");
        }
    }

    #[test]
    fn builds_tree_explain_analyze_only_for_side_effect_free_reads() {
        for query in ["SELECT * FROM users", "TABLE users"] {
            assert_eq!(
                build_explain_analyze_sql(query),
                Some(format!("EXPLAIN ANALYZE FORMAT=TREE {query}")),
                "{query}"
            );
        }

        for query in [
            "UPDATE users SET name = 'Ada' WHERE id = 1",
            "DELETE FROM users WHERE id = 1",
            "INSERT INTO users VALUES (1)",
            "REPLACE INTO users VALUES (1)",
            "SELECT * FROM users FOR UPDATE",
            "SELECT 1; SELECT 2",
        ] {
            assert_eq!(build_explain_analyze_sql(query), None, "{query}");
        }
    }
}
