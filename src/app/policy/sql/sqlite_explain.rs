use crate::policy::sql::statement_classifier::{self, StatementKind};

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
    matches!(
        statement_classifier::classify(statement),
        StatementKind::Select
            | StatementKind::Insert
            | StatementKind::Update { .. }
            | StatementKind::Delete { .. }
    ) || statement_classifier::first_keyword(statement)
        .is_some_and(|keyword| keyword.eq_ignore_ascii_case("REPLACE"))
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
    if statement_classifier::first_keyword(trimmed)
        .is_some_and(|keyword| keyword.eq_ignore_ascii_case("EXPLAIN"))
    {
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
    fn wraps_select_with_query_plan() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("SELECT 1"),
            Some("EXPLAIN QUERY PLAN SELECT 1".to_string())
        );
    }

    #[test]
    fn passes_through_existing_query_plan_prefix() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("EXPLAIN QUERY PLAN SELECT * FROM users"),
            Some("EXPLAIN QUERY PLAN SELECT * FROM users".to_string())
        );
    }

    #[test]
    fn wraps_dml_with_query_plan() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("UPDATE users SET name = 'a' WHERE id = 1"),
            Some("EXPLAIN QUERY PLAN UPDATE users SET name = 'a' WHERE id = 1".to_string())
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql("DELETE FROM users"),
            Some("EXPLAIN QUERY PLAN DELETE FROM users".to_string())
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql(
                "INSERT INTO users(name) SELECT name FROM old_users"
            ),
            Some(
                "EXPLAIN QUERY PLAN INSERT INTO users(name) SELECT name FROM old_users".to_string()
            )
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql("REPLACE INTO users(id) VALUES (1)"),
            Some("EXPLAIN QUERY PLAN REPLACE INTO users(id) VALUES (1)".to_string())
        );
    }

    #[test]
    fn rejects_prefixed_explain_and_ddl() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("EXPLAIN SELECT 1"),
            None
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql("CREATE TABLE users(id INTEGER)"),
            None
        );
    }

    #[test]
    fn rejects_query_plan_prefix_without_boundary() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("EXPLAIN QUERY PLANSELECT 1"),
            None
        );
    }

    #[test]
    fn passes_through_query_plan_with_sql_comment_after_prefix() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("EXPLAIN QUERY PLAN -- note\nSELECT 1"),
            Some("EXPLAIN QUERY PLAN -- note\nSELECT 1".to_string())
        );
    }

    #[test]
    fn prefix_check_does_not_panic_at_non_char_boundary() {
        let input = format!("EXPLAIN QUERY PLA{} SELECT 1", '\u{1F600}');
        let _ = build_sqlite_explain_query_plan_sql(&input);
    }

    #[test]
    fn leading_text_before_query_plan_prefix_is_not_treated_as_prefix() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("😀 EXPLAIN QUERY PLAN SELECT 1"),
            None
        );
    }

    #[test]
    fn detects_query_plan_prefix() {
        assert!(is_sqlite_explain_query_plan_sql(
            "EXPLAIN QUERY PLAN SELECT 1"
        ));
        assert!(!is_sqlite_explain_query_plan_sql(
            "EXPLAIN QUERY PLANSELECT 1"
        ));
    }

    #[test]
    fn multiline_select_with_leading_sql_comment_is_wrapped() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("-- filter\nSELECT 1"),
            Some("EXPLAIN QUERY PLAN -- filter\nSELECT 1".to_string())
        );
    }
}
