use super::splitter::{
    first_sqlite_keyword, has_cte_body_starting_with, split_sqlite_statements, statement_keyword,
    top_level_keywords,
};

pub const SQLITE_EXPLAIN_QUERY_PLAN_PREFIX: &str = "EXPLAIN QUERY PLAN";

fn is_valid_explain_query_plan_boundary(rest: &str) -> bool {
    if rest.is_empty() {
        return false;
    }
    let first = rest.as_bytes()[0];
    first.is_ascii_whitespace() || rest.starts_with("--") || rest.starts_with("/*")
}

fn strip_sqlite_explain_query_plan_prefix(trimmed: &str) -> Option<&str> {
    let prefix = SQLITE_EXPLAIN_QUERY_PLAN_PREFIX;
    trimmed
        .get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .and_then(|_| trimmed.get(prefix.len()..))
        .filter(|rest| is_valid_explain_query_plan_boundary(rest))
        .map(str::trim_start)
}

pub fn is_sqlite_explain_query_plan_sql(query: &str) -> bool {
    strip_sqlite_explain_query_plan_prefix(query.trim()).is_some()
}

fn supports_sqlite_query_plan(statement: &str) -> bool {
    if split_sqlite_statements(statement).statements().len() != 1 {
        return false;
    }
    if has_cte_body_starting_with(statement, "MERGE") {
        return false;
    }
    let effective_keyword = statement_keyword(statement);
    if matches!(effective_keyword.as_deref(), Some("SELECT" | "SHOW"))
        && top_level_keywords(statement)
            .iter()
            .skip(1)
            .any(|keyword| keyword == "INTO")
    {
        return false;
    }
    matches!(
        effective_keyword.as_deref(),
        Some("SELECT" | "SHOW" | "INSERT" | "UPDATE" | "DELETE")
    ) || (effective_keyword.as_deref() == Some("REPLACE")
        && first_sqlite_keyword(statement).as_deref() == Some("REPLACE"))
}

pub fn build_sqlite_explain_query_plan_sql(query: &str) -> Option<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(inner) = strip_sqlite_explain_query_plan_prefix(trimmed) {
        if supports_sqlite_query_plan(inner) {
            return Some(trimmed.to_string());
        }
        return None;
    }
    if first_sqlite_keyword(trimmed).as_deref() == Some("EXPLAIN") {
        return None;
    }
    if !supports_sqlite_query_plan(trimmed) {
        return None;
    }
    Some(format!("{SQLITE_EXPLAIN_QUERY_PLAN_PREFIX} {trimmed}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_read_and_write_queries_with_query_plan() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("SELECT 1"),
            Some("EXPLAIN QUERY PLAN SELECT 1".to_string())
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql("REPLACE INTO users(id) VALUES (1)"),
            Some("EXPLAIN QUERY PLAN REPLACE INTO users(id) VALUES (1)".to_string())
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql("SHOW tables"),
            Some("EXPLAIN QUERY PLAN SHOW tables".to_string())
        );
    }

    #[test]
    fn preserves_valid_query_plan_and_rejects_other_explain_forms() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("EXPLAIN QUERY PLAN SELECT * FROM users"),
            Some("EXPLAIN QUERY PLAN SELECT * FROM users".to_string())
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql("EXPLAIN SELECT 1"),
            None
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql("EXPLAIN QUERY PLANSELECT 1"),
            None
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql("SELECT * INTO backup FROM users"),
            None
        );
    }

    #[test]
    fn handles_cte_queries_and_comments() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("WITH rows AS (SELECT 1) SELECT * FROM rows"),
            Some("EXPLAIN QUERY PLAN WITH rows AS (SELECT 1) SELECT * FROM rows".to_string())
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql(
                "WITH \"rows\" AS (SELECT 1) SELECT * FROM \"rows\""
            ),
            Some(
                "EXPLAIN QUERY PLAN WITH \"rows\" AS (SELECT 1) SELECT * FROM \"rows\"".to_string()
            )
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql(
                "WITH payload(id) AS (VALUES (1)) REPLACE INTO users(id) SELECT id FROM payload"
            ),
            None
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql(concat!(
                "WITH x AS (MERGE INTO users USING incoming ON users.id = incoming.id ",
                "WHEN MATCHED THEN UPDATE SET name = incoming.name RETURNING *) ",
                "SELECT * FROM x"
            )),
            None
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql("WITH x AS (SELECT merge FROM t) SELECT * FROM x"),
            Some("EXPLAIN QUERY PLAN WITH x AS (SELECT merge FROM t) SELECT * FROM x".to_string())
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql("-- filter\nSELECT 1"),
            Some("EXPLAIN QUERY PLAN -- filter\nSELECT 1".to_string())
        );
    }

    #[test]
    fn detects_query_plan_prefix_only_at_a_valid_boundary() {
        assert!(is_sqlite_explain_query_plan_sql(
            "EXPLAIN QUERY PLAN SELECT 1"
        ));
        assert!(!is_sqlite_explain_query_plan_sql(
            "EXPLAIN QUERY PLANSELECT 1"
        ));
    }
}
