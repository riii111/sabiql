use super::*;

mod command_tags {
    use super::*;

    #[tokio::test]
    async fn values_result_does_not_get_select_command_tag() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(&dsn, "VALUES (1)", AccessMode::ReadOnly)
            .await
            .unwrap();

        assert_eq!(test_support::display_row(&result, 0), vec!["1".to_string()]);
        assert_eq!(result.command_tag, None);
    }

    #[tokio::test]
    async fn dml_returns_affected_rows_command_tag() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
        );
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "UPDATE users SET name = 'x' WHERE id = 1",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(result.row_count(), 1);
        assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
    }

    #[tokio::test]
    async fn replace_into_returns_insert_refresh_tag() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
        );
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "REPLACE INTO users(id, name) VALUES (1, 'z')",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(result.row_count(), 1);
        assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
    }

    #[tokio::test]
    async fn dml_with_following_select_uses_trailing_changes_result() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
        );
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "UPDATE users SET name = 'x' WHERE id = 1; SELECT 42",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(result.row_count(), 1);
        assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
    }

    #[tokio::test]
    async fn dml_with_following_select_preserves_result_set_and_refresh_tag() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
        );
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "UPDATE users SET name = 'x' WHERE id = 1; SELECT name FROM users",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["name"]);
        assert_eq!(test_support::display_row(&result, 0), vec!["x".to_string()]);
        assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
    }

    #[tokio::test]
    async fn multi_dml_uses_last_effective_refresh_tag() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
        );
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "INSERT INTO users(id, name) VALUES (2, 'b'), (3, 'c');
                     UPDATE users SET name = 'z' WHERE id IN (1, 2);
                     DELETE FROM users WHERE id = 3",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(result.row_count(), 1);
        assert_eq!(result.command_tag, Some(CommandTag::Delete(1)));
    }

    #[tokio::test]
    async fn ddl_wins_over_later_dml_for_refresh_tag() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "CREATE TABLE users(id INTEGER PRIMARY KEY);
                     INSERT INTO users(id) VALUES (1)",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(
            result.command_tag,
            Some(CommandTag::Create("TABLE".to_string()))
        );
        assert_eq!(result.row_count(), 0);
    }
}

mod transaction_execution {
    use super::*;

    mod automatic_transactions {
        use super::*;

        #[tokio::test]
        async fn ddl_and_dml_still_roll_back_as_one_auto_transaction() {
            let (_dir, dsn) = test_support::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "CREATE TABLE users(id INTEGER PRIMARY KEY);\
                     INSERT INTO users(id) VALUES (1);\
                     INSERT INTO missing(id) VALUES (2)",
                    AccessMode::ReadWrite,
                )
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            let tables = adapter
                .execute_adhoc(
                    &dsn,
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'users'",
                    AccessMode::ReadOnly,
                )
                .await
                .unwrap();
            assert_eq!(tables.data_row_count(), 0);
        }

        #[tokio::test]
        async fn persistent_pragma_writes_roll_back_when_a_later_statement_fails() {
            for (write, read) in [
                ("PRAGMA user_version = 42", "PRAGMA user_version"),
                (
                    "PRAGMA \"main\".\"user_version\" = 42",
                    "PRAGMA user_version",
                ),
                ("PRAGMA [main].[application_id](7)", "PRAGMA application_id"),
            ] {
                let (_dir, dsn) = test_support::make_sqlite_db("");
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        &format!("{write}; SELECT * FROM missing_table"),
                        AccessMode::ReadWrite,
                    )
                    .await;

                assert!(
                    matches!(result, Err(DbOperationError::ObjectMissing(_))),
                    "{write}"
                );
                let value = adapter
                    .execute_adhoc(&dsn, read, AccessMode::ReadOnly)
                    .await
                    .unwrap();
                assert_eq!(
                    test_support::display_row(&value, 0),
                    vec!["0".to_string()],
                    "{write}"
                );
            }
        }

        #[tokio::test]
        async fn vacuum_is_rejected_in_safe_mode() {
            let (_dir, dsn) = test_support::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "VACUUM", AccessMode::ReadWrite)
                .await;

            // VACUUM internally attaches a temporary database, so safe mode reports ATTACH.
            assert!(matches!(
                result,
                Err(DbOperationError::QueryFailed(details))
                    if details.contains("cannot run ATTACH in safe mode")
            ));
        }

        #[tokio::test]
        async fn journal_mode_change_in_mixed_sql_runs_outside_auto_transaction() {
            let (_dir, dsn) = test_support::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            adapter
                .execute_adhoc(
                    &dsn,
                    "PRAGMA journal_mode = WAL;\
                     CREATE TABLE users(id INTEGER PRIMARY KEY);\
                     INSERT INTO users(id) VALUES (1)",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();

            assert_eq!(test_support::display_row(&rows, 0), vec!["1".to_string()]);
        }

        #[tokio::test]
        async fn foreign_keys_change_in_mixed_sql_is_not_a_transaction_noop() {
            let (_dir, dsn) = test_support::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "PRAGMA foreign_keys = ON;
                     CREATE TABLE parent(id INTEGER PRIMARY KEY);
                     PRAGMA foreign_keys",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();

            assert_eq!(test_support::display_row(&result, 0), vec!["1".to_string()]);
        }
    }

    mod savepoint_rollbacks {
        use super::*;

        #[tokio::test]
        async fn rolled_back_dml_returns_rollback_tag() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "BEGIN; INSERT INTO users(id) VALUES (1); ROLLBACK",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Rollback));
            assert_eq!(rows.data_row_count(), 0);
        }

        #[tokio::test]
        async fn full_rollback_inside_savepoint_discards_outer_dml() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT sp;
                     INSERT INTO users(id) VALUES (2);
                     ROLLBACK",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Rollback));
            assert_eq!(rows.data_row_count(), 0);
        }

        #[tokio::test]
        async fn savepoint_rollback_discards_inner_dml_only() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT sp;
                     INSERT INTO users(id) VALUES (2);
                     ROLLBACK TO sp;
                     COMMIT",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            assert_eq!(test_support::display_row(&rows, 0), vec!["1".to_string()]);
        }

        #[tokio::test]
        async fn rollback_to_keeps_savepoint_for_later_rollback() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT sp;
                     INSERT INTO users(id) VALUES (2);
                     ROLLBACK TO sp;
                     INSERT INTO users(id) VALUES (3);
                     ROLLBACK TO sp;
                     COMMIT",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            assert_eq!(test_support::display_row(&rows, 0), vec!["1".to_string()]);
        }

        #[tokio::test]
        async fn rollback_to_named_outer_savepoint_discards_nested_frames() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT outer_sp;
                     INSERT INTO users(id) VALUES (2);
                     SAVEPOINT inner_sp;
                     INSERT INTO users(id) VALUES (3);
                     ROLLBACK TO outer_sp;
                     COMMIT",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            assert_eq!(test_support::display_row(&rows, 0), vec!["1".to_string()]);
        }

        #[tokio::test]
        async fn top_level_savepoint_rollback_to_discards_inner_dml_only() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SAVEPOINT sp;
                     INSERT INTO users(id) VALUES (1);
                     INSERT INTO users(id) VALUES (2);
                     ROLLBACK TO sp;
                     INSERT INTO users(id) VALUES (3);
                     RELEASE sp",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            assert_eq!(test_support::display_row(&rows, 0), vec!["3".to_string()]);
        }

        #[tokio::test]
        async fn top_level_savepoint_multi_write_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
            .execute_adhoc(
                &dsn,
                "SAVEPOINT sp; INSERT INTO users(id) VALUES (1); INSERT INTO missing(id) VALUES (2)", AccessMode::ReadWrite)
            .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();
            assert_eq!(rows.data_row_count(), 0);
        }

        #[tokio::test]
        async fn top_level_savepoint_without_release_does_not_persist_on_success() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
            .execute_adhoc(
                &dsn,
                "SAVEPOINT sp; INSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2)", AccessMode::ReadWrite)
            .await
            .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Rollback));
            assert_eq!(rows.data_row_count(), 0);
        }
    }

    mod multi_statement_atomicity {
        use super::*;

        #[tokio::test]
        async fn multi_statement_dml_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "INSERT INTO users(id) VALUES (1); INSERT INTO missing(id) VALUES (2)",
                    AccessMode::ReadWrite,
                )
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();
            assert_eq!(rows.data_row_count(), 0);
        }

        #[tokio::test]
        async fn with_dml_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "WITH payload(id) AS (VALUES (1))
                     INSERT INTO users(id) SELECT id FROM payload;
                     INSERT INTO missing(id) VALUES (2)",
                    AccessMode::ReadWrite,
                )
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();
            assert_eq!(rows.data_row_count(), 0);
        }

        #[tokio::test]
        async fn returning_dml_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();
            let query =
                "INSERT INTO users(id) VALUES (1) RETURNING id; INSERT INTO missing(id) VALUES (2)";

            let result = adapter
                .execute_adhoc(&dsn, query, AccessMode::ReadWrite)
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();
            assert_eq!(rows.data_row_count(), 0);
        }

        #[tokio::test]
        async fn select_then_dml_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
            .execute_adhoc(
                &dsn,
                "SELECT 1 AS marker; INSERT INTO users(id) VALUES (1); INSERT INTO missing(id) VALUES (2)", AccessMode::ReadWrite)
            .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();
            assert_eq!(rows.data_row_count(), 0);
        }
    }
}

mod returning_results {
    use super::*;

    #[tokio::test]
    async fn dml_returning_preserves_returned_rows() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "INSERT INTO users(name) VALUES ('a') RETURNING id, name",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["id", "name"]);
        assert_eq!(
            test_support::display_row(&result, 0),
            vec!["1".to_string(), "a".to_string()]
        );
        assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
    }

    #[tokio::test]
    async fn dml_returning_preserves_empty_suffix_column() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "INSERT INTO users(name) VALUES ('a') RETURNING id AS value_empty",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["value_empty"]);
        assert_eq!(test_support::display_row(&result, 0), vec!["1".to_string()]);
        assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
    }

    #[tokio::test]
    async fn update_returning_preserves_returned_rows() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
        );
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "UPDATE users SET name = 'x' RETURNING id, name",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(result.data_row_count(), 2);
        assert_eq!(result.command_tag, Some(CommandTag::Update(2)));
    }

    #[tokio::test]
    async fn delete_returning_preserves_returned_rows() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
        );
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "DELETE FROM users WHERE id = 1 RETURNING id, name",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(
            test_support::display_row(&result, 0),
            vec!["1".to_string(), "a".to_string()]
        );
        assert_eq!(result.command_tag, Some(CommandTag::Delete(1)));
    }
}

mod dml_command_tags {
    use super::*;

    #[tokio::test]
    async fn dml_with_trailing_line_comment_returns_affected_rows() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
        );
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "DELETE FROM users WHERE id = 1 -- cleanup selected row",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(result.row_count(), 1);
        assert_eq!(result.command_tag, Some(CommandTag::Delete(1)));
    }

    #[tokio::test]
    async fn with_insert_reports_affected_rows_command_tag() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "WITH payload(id) AS (VALUES (1), (2))
                     INSERT INTO users(id) SELECT id FROM payload",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(result.row_count(), 2);
        assert_eq!(result.command_tag, Some(CommandTag::Insert(2)));
    }

    #[rstest::rstest]
    #[case::unquoted("returning_log")]
    #[case::backtick_quoted("`my returning`")]
    #[case::bracket_quoted("[my returning]")]
    #[tokio::test]
    async fn identifier_containing_returning_reports_affected_rows(#[case] identifier: &str) {
        let (_dir, dsn) = test_support::make_sqlite_db(&format!(
            "CREATE TABLE {identifier}(id INTEGER PRIMARY KEY, name TEXT);"
        ));
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                &format!("INSERT INTO {identifier}(name) VALUES ('a')"),
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(result.row_count(), 1);
        assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
    }
}

mod ddl_command_tags {
    use super::*;

    #[tokio::test]
    async fn ddl_returns_schema_refresh_command_tag() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "CREATE TABLE users(id INTEGER PRIMARY KEY)",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(
            result.command_tag,
            Some(CommandTag::Create("TABLE".to_string()))
        );
    }
}
