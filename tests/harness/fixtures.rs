use std::time::Instant;

use dbtui::domain::{Column, DatabaseMetadata, QueryResult, QuerySource, Table, TableSummary};

pub fn sample_metadata(now: Instant) -> DatabaseMetadata {
    DatabaseMetadata {
        database_name: "test_db".to_string(),
        schemas: vec![],
        tables: vec![
            TableSummary::new("public".to_string(), "users".to_string(), Some(100), false),
            TableSummary::new("public".to_string(), "posts".to_string(), Some(50), false),
            TableSummary::new(
                "public".to_string(),
                "comments".to_string(),
                Some(200),
                false,
            ),
        ],
        fetched_at: now,
    }
}

pub fn sample_table_detail() -> Table {
    Table {
        schema: "public".to_string(),
        name: "users".to_string(),
        columns: vec![
            Column {
                name: "id".to_string(),
                data_type: "integer".to_string(),
                nullable: false,
                is_primary_key: true,
                is_unique: true,
                default: None,
                comment: None,
                ordinal_position: 1,
            },
            Column {
                name: "name".to_string(),
                data_type: "varchar(255)".to_string(),
                nullable: false,
                is_primary_key: false,
                is_unique: false,
                default: None,
                comment: None,
                ordinal_position: 2,
            },
            Column {
                name: "email".to_string(),
                data_type: "varchar(255)".to_string(),
                nullable: true,
                is_primary_key: false,
                is_unique: true,
                default: None,
                comment: None,
                ordinal_position: 3,
            },
        ],
        primary_key: Some(vec!["id".to_string()]),
        indexes: vec![],
        foreign_keys: vec![],
        rls: None,
        row_count_estimate: Some(100),
        comment: None,
    }
}

pub fn sample_query_result(now: Instant) -> QueryResult {
    QueryResult {
        query: "SELECT * FROM users LIMIT 100".to_string(),
        columns: vec!["id".to_string(), "name".to_string(), "email".to_string()],
        rows: vec![
            vec![
                "1".to_string(),
                "Alice".to_string(),
                "alice@example.com".to_string(),
            ],
            vec![
                "2".to_string(),
                "Bob".to_string(),
                "bob@example.com".to_string(),
            ],
        ],
        row_count: 2,
        execution_time_ms: 15,
        executed_at: now,
        source: QuerySource::Preview,
        error: None,
    }
}

pub fn empty_query_result(now: Instant) -> QueryResult {
    QueryResult {
        query: "SELECT * FROM users WHERE 1=0".to_string(),
        columns: vec!["id".to_string(), "name".to_string(), "email".to_string()],
        rows: vec![],
        row_count: 0,
        execution_time_ms: 5,
        executed_at: now,
        source: QuerySource::Preview,
        error: None,
    }
}
