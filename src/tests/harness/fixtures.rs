use sabiql_app::policy::write::write_guardrails::{GuardrailDecision, RiskLevel};
use sabiql_domain::{
    Column, ColumnAttributes, DatabaseMetadata, DiagnosticField, FkAction, ForeignKey, Index,
    IndexAttributes, IndexType, QueryResult, QuerySource, SqliteDiagnosticsSnapshot, Table,
    TableSummary, Trigger, TriggerEvent, TriggerTiming,
};

pub fn sample_metadata() -> DatabaseMetadata {
    let mut metadata = DatabaseMetadata::new("test_db".to_string());
    metadata.table_summaries = vec![
        TableSummary::new("public".to_string(), "users".to_string(), Some(100), false),
        TableSummary::new("public".to_string(), "posts".to_string(), Some(50), false),
        TableSummary::new(
            "public".to_string(),
            "comments".to_string(),
            Some(200),
            false,
        ),
    ];
    metadata
}

pub fn sample_table_detail() -> Table {
    let mut table = Table::minimal_for_test("public", "users");
    table.owner = Some("postgres".to_string());
    table.columns = vec![
        Column {
            name: "id".to_string(),
            data_type: "integer".to_string(),
            attributes: ColumnAttributes::PRIMARY_KEY | ColumnAttributes::UNIQUE,
            default: None,
            comment: Some("Primary key".to_string()),
            ordinal_position: 1,
        },
        Column {
            name: "name".to_string(),
            data_type: "varchar(255)".to_string(),
            attributes: ColumnAttributes::empty(),
            default: None,
            comment: None,
            ordinal_position: 2,
        },
        Column {
            name: "email".to_string(),
            data_type: "varchar(255)".to_string(),
            attributes: ColumnAttributes::NULLABLE | ColumnAttributes::UNIQUE,
            default: None,
            comment: None,
            ordinal_position: 3,
        },
    ];
    table.primary_key = Some(vec!["id".to_string()]);
    table.indexes = vec![
        Index {
            name: "users_pkey".to_string(),
            columns: vec!["id".to_string()],
            attributes: IndexAttributes::UNIQUE | IndexAttributes::PRIMARY,
            index_type: IndexType::BTree,
            definition: None,
        },
        Index {
            name: "idx_users_email".to_string(),
            columns: vec!["email".to_string()],
            attributes: IndexAttributes::UNIQUE,
            index_type: IndexType::BTree,
            definition: None,
        },
    ];
    table.foreign_keys = vec![ForeignKey {
        name: "fk_users_department".to_string(),
        from_schema: "public".to_string(),
        from_table: "users".to_string(),
        from_columns: vec!["department_id".to_string()],
        to_schema: "public".to_string(),
        to_table: "departments".to_string(),
        to_columns: vec!["id".to_string()],
        on_delete: FkAction::Cascade,
        on_update: FkAction::NoAction,
        reference_resolved: true,
    }];
    table.triggers = vec![Trigger {
        name: "audit_users".to_string(),
        timing: TriggerTiming::After,
        events: vec![TriggerEvent::Insert, TriggerEvent::Update],
        function_name: "audit_func".to_string(),
        security_definer: false,
    }];
    table.row_count_estimate = Some(100);
    table.comment = Some("User accounts".to_string());
    table
}

pub fn loaded_sqlite_diagnostics() -> SqliteDiagnosticsSnapshot {
    SqliteDiagnosticsSnapshot {
        db_file: DiagnosticField::ok("/tmp/app.db"),
        sqlite_version: DiagnosticField::ok("3.45.0"),
        foreign_keys: DiagnosticField::ok("on"),
        journal_mode: DiagnosticField::ok("wal"),
        query_only: DiagnosticField::ok("off"),
        busy_timeout: DiagnosticField::ok("5000"),
        database_list: DiagnosticField::ok("0: main @ /tmp/app.db"),
        quick_check: DiagnosticField::ok("ok"),
    }
}

pub fn sample_query_result() -> QueryResult {
    QueryResult::success(
        "SELECT * FROM users LIMIT 100".to_string(),
        vec!["id".to_string(), "name".to_string(), "email".to_string()],
        vec![
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
        15,
        QuerySource::Preview,
    )
}

pub fn empty_query_result() -> QueryResult {
    QueryResult::success(
        "SELECT * FROM users WHERE 1=0".to_string(),
        vec!["id".to_string(), "name".to_string(), "email".to_string()],
        vec![],
        5,
        QuerySource::Preview,
    )
}

pub fn low_risk_guardrail() -> GuardrailDecision {
    GuardrailDecision {
        risk_level: RiskLevel::Low,
        blocked: false,
        reason: None,
        target_summary: None,
    }
}
