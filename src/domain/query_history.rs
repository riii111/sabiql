use serde::{Deserialize, Serialize};

use super::connection::ConnectionId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Iso8601Timestamp(String);

impl Iso8601Timestamp {
    pub fn new(s: String) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Iso8601Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryResultStatus {
    Success,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlCategory {
    Select,
    Dml,
    Ddl,
    Tcl,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryHistoryEntry {
    pub query: String,
    pub executed_at: Iso8601Timestamp,
    pub connection_id: ConnectionId,
    pub result_status: QueryResultStatus,
    pub affected_rows: Option<u64>,
}

impl QueryHistoryEntry {
    pub fn new(
        query: String,
        executed_at: String,
        connection_id: ConnectionId,
        result_status: QueryResultStatus,
        affected_rows: Option<u64>,
    ) -> Self {
        Self {
            query,
            executed_at: Iso8601Timestamp::new(executed_at),
            connection_id,
            result_status,
            affected_rows,
        }
    }
}

pub fn classify_sql(query: &str) -> SqlCategory {
    let first_keyword = skip_comments_and_first_word(query);
    match first_keyword.to_uppercase().as_str() {
        "SELECT" | "TABLE" => SqlCategory::Select,
        "INSERT" | "UPDATE" | "DELETE" | "MERGE" | "UPSERT" | "COPY" => SqlCategory::Dml,
        "CREATE" | "ALTER" | "DROP" | "TRUNCATE" | "COMMENT" | "GRANT" | "REVOKE" => {
            SqlCategory::Ddl
        }
        "BEGIN" | "START" | "COMMIT" | "ROLLBACK" | "SAVEPOINT" | "RELEASE" | "SET" => {
            SqlCategory::Tcl
        }
        "WITH" => classify_with_body(query),
        "EXPLAIN" | "ANALYZE" => classify_after_explain(query),
        _ => SqlCategory::Other,
    }
}

fn skip_comments_and_first_word(s: &str) -> String {
    let mut chars = s.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek() == Some(&'-') {
            let mut clone = chars.clone();
            clone.next();
            if clone.peek() == Some(&'-') {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }
        }
        if chars.peek() == Some(&'/') {
            let mut clone = chars.clone();
            clone.next();
            if clone.peek() == Some(&'*') {
                chars.next();
                chars.next();
                let mut depth = 1u32;
                while depth > 0 {
                    match chars.next() {
                        Some('*') if chars.peek() == Some(&'/') => {
                            chars.next();
                            depth -= 1;
                        }
                        Some('/') if chars.peek() == Some(&'*') => {
                            chars.next();
                            depth += 1;
                        }
                        None => break,
                        _ => {}
                    }
                }
                continue;
            }
        }
        break;
    }
    let mut word = String::new();
    for c in chars {
        if c.is_whitespace() || c == '(' || c == ';' {
            break;
        }
        word.push(c);
    }
    word
}

fn classify_with_body(query: &str) -> SqlCategory {
    let upper = query.to_uppercase();
    for keyword in ["INSERT", "UPDATE", "DELETE", "MERGE"] {
        if upper.contains(keyword) {
            return SqlCategory::Dml;
        }
    }
    SqlCategory::Select
}

fn classify_after_explain(query: &str) -> SqlCategory {
    let upper = query.to_uppercase();
    for keyword in ["SELECT", "WITH"] {
        if upper.contains(keyword) {
            return SqlCategory::Select;
        }
    }
    for keyword in ["INSERT", "UPDATE", "DELETE", "MERGE"] {
        if upper.contains(keyword) {
            return SqlCategory::Dml;
        }
    }
    for keyword in ["CREATE", "ALTER", "DROP"] {
        if upper.contains(keyword) {
            return SqlCategory::Ddl;
        }
    }
    SqlCategory::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip() {
        let entry = QueryHistoryEntry::new(
            "SELECT * FROM users".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-uuid"),
            QueryResultStatus::Success,
            None,
        );

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: QueryHistoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry, deserialized);
    }

    #[test]
    fn serde_round_trip_with_affected_rows() {
        let entry = QueryHistoryEntry::new(
            "UPDATE users SET name = 'x'".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-uuid"),
            QueryResultStatus::Success,
            Some(5),
        );

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: QueryHistoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry, deserialized);
        assert_eq!(deserialized.result_status, QueryResultStatus::Success);
        assert_eq!(deserialized.affected_rows, Some(5));
    }

    #[test]
    fn serde_json_format() {
        let entry = QueryHistoryEntry::new(
            "SELECT 1".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("abc-123"),
            QueryResultStatus::Success,
            None,
        );

        let json = serde_json::to_string(&entry).unwrap();

        assert!(json.contains("\"query\":\"SELECT 1\""));
        assert!(json.contains("\"executed_at\":\"2026-03-13T12:00:00Z\""));
        assert!(json.contains("\"connection_id\":\"abc-123\""));
        assert!(json.contains("\"result_status\":\"Success\""));
    }

    mod classify {
        use super::*;

        #[test]
        fn empty_string() {
            assert_eq!(classify_sql(""), SqlCategory::Other);
        }

        #[test]
        fn select() {
            assert_eq!(classify_sql("SELECT * FROM users"), SqlCategory::Select);
        }

        #[test]
        fn table_command() {
            assert_eq!(classify_sql("TABLE users"), SqlCategory::Select);
        }

        #[test]
        fn case_insensitive() {
            assert_eq!(classify_sql("select 1"), SqlCategory::Select);
            assert_eq!(classify_sql("Select 1"), SqlCategory::Select);
        }

        #[test]
        fn leading_whitespace() {
            assert_eq!(classify_sql("  \n  SELECT 1"), SqlCategory::Select);
        }

        #[test]
        fn dml_keywords() {
            assert_eq!(classify_sql("INSERT INTO t VALUES (1)"), SqlCategory::Dml);
            assert_eq!(classify_sql("UPDATE t SET x = 1"), SqlCategory::Dml);
            assert_eq!(classify_sql("DELETE FROM t"), SqlCategory::Dml);
        }

        #[test]
        fn ddl_keywords() {
            assert_eq!(classify_sql("CREATE TABLE t (id int)"), SqlCategory::Ddl);
            assert_eq!(classify_sql("ALTER TABLE t ADD col int"), SqlCategory::Ddl);
            assert_eq!(classify_sql("DROP TABLE t"), SqlCategory::Ddl);
        }

        #[test]
        fn tcl_keywords() {
            assert_eq!(classify_sql("BEGIN"), SqlCategory::Tcl);
            assert_eq!(classify_sql("COMMIT"), SqlCategory::Tcl);
            assert_eq!(classify_sql("ROLLBACK"), SqlCategory::Tcl);
        }

        #[test]
        fn line_comment_prefix() {
            assert_eq!(classify_sql("-- comment\nSELECT 1"), SqlCategory::Select);
        }

        #[test]
        fn block_comment_prefix() {
            assert_eq!(classify_sql("/* comment */ SELECT 1"), SqlCategory::Select);
        }

        #[test]
        fn nested_block_comment() {
            assert_eq!(
                classify_sql("/* outer /* inner */ end */ SELECT 1"),
                SqlCategory::Select
            );
        }

        #[test]
        fn with_select() {
            assert_eq!(
                classify_sql("WITH cte AS (SELECT 1) SELECT * FROM cte"),
                SqlCategory::Select
            );
        }

        #[test]
        fn with_insert() {
            assert_eq!(
                classify_sql("WITH cte AS (SELECT 1) INSERT INTO t SELECT * FROM cte"),
                SqlCategory::Dml
            );
        }

        #[test]
        fn explain_select() {
            assert_eq!(classify_sql("EXPLAIN SELECT * FROM t"), SqlCategory::Select);
        }

        #[test]
        fn explain_analyze_select() {
            assert_eq!(
                classify_sql("EXPLAIN ANALYZE SELECT * FROM t"),
                SqlCategory::Select
            );
        }
    }
}
