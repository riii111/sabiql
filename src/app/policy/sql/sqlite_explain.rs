use crate::policy::sql::statement_classifier::{self, StatementKind};

pub const SQLITE_EXPLAIN_QUERY_PLAN_PREFIX: &str = "EXPLAIN QUERY PLAN";

fn strip_sqlite_explain_query_plan_prefix(trimmed: &str) -> Option<&str> {
    let prefix = SQLITE_EXPLAIN_QUERY_PLAN_PREFIX;
    trimmed
        .get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .and_then(|_| trimmed.get(prefix.len()..))
        .map(str::trim_start)
}

pub fn build_sqlite_explain_query_plan_sql(query: &str) -> Option<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(inner) = strip_sqlite_explain_query_plan_prefix(trimmed) {
        if matches!(statement_classifier::classify(inner), StatementKind::Select) {
            return Some(trimmed.to_string());
        }
        return None;
    }
    if statement_classifier::first_keyword(trimmed)
        .is_some_and(|keyword| keyword.eq_ignore_ascii_case("EXPLAIN"))
    {
        return None;
    }
    if !matches!(
        statement_classifier::classify(trimmed),
        StatementKind::Select
    ) {
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
    fn rejects_non_select_and_prefixed_explain() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("DELETE FROM users"),
            None
        );
        assert_eq!(
            build_sqlite_explain_query_plan_sql("EXPLAIN SELECT 1"),
            None
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
    fn multiline_select_with_leading_emoji_line_is_wrapped() {
        assert_eq!(
            build_sqlite_explain_query_plan_sql("😀\nSELECT 1"),
            Some("EXPLAIN QUERY PLAN 😀\nSELECT 1".to_string())
        );
    }
}
