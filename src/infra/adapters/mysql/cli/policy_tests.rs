#[cfg(test)]
mod policy_tests {
    use crate::app::ports::outbound::MYSQL_SQL_MODE_UNSUPPORTED_MARKER;

    use super::super::super::error::validate_mode_probe;
    use super::super::*;

    #[test]
    fn csv_export_accepts_one_read_only_result_query() {
        assert!(validate_mysql_export_query("SELECT 1", Some("app")).is_ok());
        for query in ["TABLE users", "SHOW TABLES", "DESCRIBE users"] {
            assert!(
                validate_mysql_export_query(query, Some("app")).is_ok(),
                "{query}"
            );
        }
        assert!(matches!(
            validate_mysql_export_query("INSERT INTO users VALUES (1)", Some("app")),
            Err(DbOperationError::PermissionDenied(_))
        ));
        assert!(matches!(
            validate_mysql_export_query("SELECT 1; SELECT 2", Some("app")),
            Err(DbOperationError::UnsupportedOperation(details))
                if details.contains("single read-only result")
        ));
    }

    #[test]
    fn mode_probe_requires_marker_and_allowed_mode_before_user_sql() {
        let probe = MysqlResultSet {
            columns: vec![
                "__sabiql_probe".to_string(),
                "__sabiql_sql_mode".to_string(),
            ],
            values: vec![vec![
                QueryValue::Text("marker".to_string()),
                QueryValue::Text("STRICT_TRANS_TABLES".to_string()),
            ]],
        };
        assert!(validate_mode_probe(&probe, "marker").is_ok());

        let mut unsupported = probe;
        unsupported.values[0][1] = QueryValue::Text("ANSI_QUOTES".to_string());
        assert!(matches!(
            validate_mode_probe(&unsupported, "marker"),
            Err(DbOperationError::UnsupportedOperation(details))
                if details.contains(MYSQL_SQL_MODE_UNSUPPORTED_MARKER)
        ));
    }
    #[test]
    fn metadata_only_select_rejects_known_side_effects() {
        for query in [
            "SELECT value FROM items FOR UPDATE",
            "SELECT GET_LOCK('sabiql', 0)",
            "SELECT @value := 1",
        ] {
            assert!(
                mysql_metadata_select_query(query, "__source", "__marker").is_err(),
                "{query}"
            );
        }
        assert!(mysql_metadata_select_query(
            "WITH cte_rows AS (SELECT 1 AS first_alias) SELECT first_alias FROM cte_rows WHERE FALSE",
            "__source",
            "__marker"
        )
        .is_ok());
        assert!(
            mysql_metadata_select_query(
                "SELECT CONCAT('a', 'b') AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT CONCAT/**/('a', 'b') AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT sabiql_test.user_function() AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT sabiql_test/**/.user_function() AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT `user_function`/**/() AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT @session_value AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT INTERVAL(10, 1, 5) AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "WITH cte_rows(first_alias) AS (SELECT 1) SELECT first_alias FROM cte_rows WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_ok()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT CASE (1) WHEN 1 THEN 'x' ELSE 'y' END AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_ok()
        );
        for query in [
            "SELECT CAST(1 AS CHAR) AS value WHERE FALSE",
            "SELECT CONVERT(1, CHAR) AS value WHERE FALSE",
            "SELECT EXTRACT(YEAR FROM CURRENT_DATE) AS value WHERE FALSE",
        ] {
            assert!(
                mysql_metadata_select_query(query, "__source", "__marker").is_err(),
                "{query}"
            );
        }
        assert!(
            mysql_metadata_select_query(
                "SELECT SLEEP(1) AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_ok()
        );
    }

    #[test]
    fn failure_before_a_change_keeps_original_error() {
        let error = query_failed_after_change(
            DbOperationError::ForeignKeyViolation("foreign key failed".to_string()),
            RefreshScope::None,
        );

        assert!(matches!(
            error,
            DbOperationError::ForeignKeyViolation(details) if details == "foreign key failed"
        ));
    }

    #[test]
    fn read_only_rejects_temporary_table_dml_before_starting_mysql() {
        let directory = tempfile::tempdir().unwrap();
        let log_file = directory.path().join("mysql.log");
        let query = "CREATE TEMPORARY TABLE temp_items (id INT); INSERT INTO temp_items VALUES (1); DROP TEMPORARY TABLE temp_items";

        let result = validate_mysql_multi_query(query, Some("app"), AccessMode::ReadOnly);

        assert!(matches!(
            result,
            Err(DbOperationError::PermissionDenied(details))
                if details.contains("read-only mode blocks MySQL write statements")
        ));
        assert!(!log_file.exists());
    }

    #[test]
    fn read_only_rejects_read_write_overrides_before_starting_mysql() {
        for query in [
            "SET SESSION TRANSACTION READ WRITE",
            "START TRANSACTION READ WRITE",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let log_file = directory.path().join("mysql.log");

            let result = validate_mysql_multi_query(query, Some("app"), AccessMode::ReadOnly);

            assert!(matches!(
                result,
                Err(DbOperationError::UnsupportedOperation(_))
            ));
            assert!(!log_file.exists(), "{query}");
        }
    }

    #[test]
    fn rejects_empty_metadata_fallback_after_temporary_table_creation() {
        for query in [
            "CREATE TEMPORARY TABLE temp_items (id INT); DESCRIBE temp_items 'missing'; DROP TEMPORARY TABLE temp_items",
            "CREATE TEMPORARY TABLE temp_items (id INT); SHOW COLUMNS FROM temp_items LIKE 'missing'; DROP TEMPORARY TABLE temp_items",
        ] {
            let statements = validate_mysql_multi_query(query, Some("app"), AccessMode::ReadWrite)
                .expect("query should be classified before the session-state check");

            assert!(mysql_metadata_fallback_has_unsupported_session_state(
                &statements
            ));
        }

        let statements = validate_mysql_multi_query(
            "SHOW COLUMNS FROM items",
            Some("app"),
            AccessMode::ReadWrite,
        )
        .expect("single SHOW should be classified");
        assert!(!mysql_metadata_fallback_has_unsupported_session_state(
            &statements
        ));
    }
    #[test]
    fn transaction_rollback_removes_pending_data_tag() {
        let events = vec![
            MysqlCommandEvent {
                kind: MysqlStatementKind::Begin,
                target: None,
                tag: CommandTag::Begin,
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Update { has_where: true },
                target: Some("items".to_string()),
                tag: CommandTag::Update(1),
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Rollback,
                target: None,
                tag: CommandTag::Rollback,
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Select,
                target: None,
                tag: CommandTag::Select(1),
            },
        ];

        assert_eq!(
            aggregate_mysql_command_tag(&events),
            Some(CommandTag::Select(1))
        );
    }

    #[test]
    fn ddl_implicit_commit_keeps_prior_data_change() {
        let events = vec![
            MysqlCommandEvent {
                kind: MysqlStatementKind::Begin,
                target: None,
                tag: CommandTag::Begin,
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Insert,
                target: Some("items".to_string()),
                tag: CommandTag::Insert(1),
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::CreateTable { temporary: false },
                target: Some("created".to_string()),
                tag: CommandTag::Create("TABLE".to_string()),
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Rollback,
                target: None,
                tag: CommandTag::Rollback,
            },
        ];

        assert_eq!(
            aggregate_mysql_command_tag(&events),
            Some(CommandTag::Create("TABLE".to_string()))
        );
    }
}
