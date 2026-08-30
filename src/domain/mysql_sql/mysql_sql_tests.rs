mod explain {
    use crate::mysql_sql::{build_explain_analyze_sql, build_explain_sql};

    #[test]
    fn builds_tree_explain_for_supported_queries() {
        for query in [
            "SELECT * FROM users",
            "TABLE users",
            "INSERT INTO users VALUES (1)",
            "REPLACE INTO users VALUES (1)",
            "REPLACE users VALUES (1)",
            "UPDATE users SET name = 'Ada' WHERE id = 1",
            "DELETE FROM users WHERE id = 1",
            "SELECT * FROM users FOR UPDATE",
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
            "REPLACE",
            "REPLACE INTO",
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
            "SELECT `GET_LOCK`('sabiql', 0)",
            "SELECT `RELEASE_LOCK`('sabiql')",
            "SELECT `RELEASE_ALL_LOCKS`()",
            "SELECT 1; SELECT 2",
        ] {
            assert_eq!(build_explain_analyze_sql(query), None, "{query}");
        }
    }
}

mod write {
    use crate::QueryValue;
    use crate::mysql_sql::{build_bulk_delete_sql, build_update_sql};

    #[test]
    fn update_quotes_identifiers_and_mysql_string_escapes() {
        let sql = build_update_sql(
            "db`name",
            "table`name",
            "value`name",
            &QueryValue::text("O'Reilly\\path\n\t\0"),
            &[
                (
                    "id`part".to_string(),
                    QueryValue::SqlLiteral("18446744073709551615".into()),
                ),
                ("tenant".to_string(), QueryValue::Null),
            ],
        );

        assert_eq!(
            sql,
            "UPDATE \x60db\x60\x60name\x60.\x60table\x60\x60name\x60\nSET \x60value\x60\x60name\x60 = 'O\\'Reilly\\\\path\\n\\t\\0'\nWHERE \x60id\x60\x60part\x60 = 18446744073709551615 AND \x60tenant\x60 IS NULL;"
        );
    }

    #[test]
    fn update_uses_text_datetime_and_blob_literals_without_coercion() {
        let sql = build_update_sql(
            "sabiql_test",
            "events",
            "payload",
            &QueryValue::Blob(vec![0, 255, 16]),
            &[(
                "created_at".to_string(),
                QueryValue::text("2026-08-13 12:34:56"),
            )],
        );

        assert_eq!(
            sql,
            "UPDATE `sabiql_test`.`events`\nSET `payload` = X'00FF10'\nWHERE `created_at` = '2026-08-13 12:34:56';"
        );
    }

    #[test]
    fn update_uses_null_literal_for_cell_value() {
        let sql = build_update_sql(
            "sabiql_test",
            "items",
            "payload",
            &QueryValue::Null,
            &[("id".to_string(), QueryValue::SqlLiteral("1".into()))],
        );

        assert_eq!(
            sql,
            "UPDATE `sabiql_test`.`items`\nSET `payload` = NULL\nWHERE `id` = 1;"
        );
    }

    #[test]
    fn json_document_update_keeps_json_null_distinct_from_string_null() {
        let json_null = build_update_sql(
            "sabiql_test",
            "documents",
            "payload",
            &QueryValue::text("null"),
            &[("id".to_string(), QueryValue::SqlLiteral("1".into()))],
        );
        let string_null = build_update_sql(
            "sabiql_test",
            "documents",
            "payload",
            &QueryValue::text(r#""null""#),
            &[("id".to_string(), QueryValue::SqlLiteral("1".into()))],
        );

        assert!(json_null.contains("SET `payload` = 'null'"));
        assert!(string_null.contains("SET `payload` = '\"null\"'"));
        assert_ne!(json_null, string_null);
    }

    #[test]
    fn bulk_delete_targets_each_composite_primary_key_row() {
        let sql = build_bulk_delete_sql(
            "sabiql_test",
            "items",
            &[
                vec![
                    ("first".to_string(), QueryValue::SqlLiteral("1".into())),
                    ("second".to_string(), QueryValue::SqlLiteral("20".into())),
                ],
                vec![
                    ("first".to_string(), QueryValue::SqlLiteral("2".into())),
                    ("second".to_string(), QueryValue::SqlLiteral("10".into())),
                ],
            ],
        );

        assert_eq!(
            sql,
            "DELETE FROM `sabiql_test`.`items`\nWHERE (`first` = 1 AND `second` = 20) OR (`first` = 2 AND `second` = 10);"
        );
    }

    #[test]
    fn bulk_delete_uses_unwrapped_predicate_for_single_row() {
        let sql = build_bulk_delete_sql(
            "sabiql_test",
            "items",
            &[vec![("id".to_string(), QueryValue::SqlLiteral("1".into()))]],
        );

        assert_eq!(sql, "DELETE FROM `sabiql_test`.`items`\nWHERE `id` = 1;");
    }

    #[test]
    #[should_panic(expected = "pk_pairs_per_row must not be empty")]
    fn bulk_delete_rejects_empty_rows() {
        let _ = build_bulk_delete_sql("sabiql_test", "items", &[]);
    }
}

mod classifier {
    use super::super::*;

    #[test]
    fn classified_statement_preserves_its_private_classifier_invariant() {
        let statement = classify_mysql_statement("DROP TABLE users").unwrap();

        assert_eq!(statement.sql(), "DROP TABLE users");
        assert_eq!(
            statement.kind(),
            &MySqlStatementKind::DropTable { temporary: false }
        );
        assert_eq!(statement.target(), Some("users"));
        assert_eq!(statement.target_database.as_deref(), None);
        assert!(validate_mysql_statements(&[statement], Some("app")).is_ok());
    }

    #[test]
    fn classifies_insert_on_duplicate_key_update_shape() {
        let upsert = classify_mysql_statement(
            "INSERT INTO users (id, name) VALUES (1, 'Ada') ON DUPLICATE KEY UPDATE name = 'Grace'",
        )
        .unwrap();
        let insert =
            classify_mysql_statement("INSERT INTO users (id, name) VALUES (1, 'Ada')").unwrap();

        assert!(upsert.has_on_duplicate_key_update());
        assert!(!insert.has_on_duplicate_key_update());
    }

    #[test]
    fn rejects_ambiguous_quotes() {
        assert!(split_mysql_statements("SELECT 'unfinished").is_err());
    }

    #[test]
    fn classifies_with_and_version_comment() {
        let statement = classify_mysql_statement(
            "WITH rows(id) AS (SELECT 1) UPDATE `app`.`items` SET value = 1",
        )
        .unwrap();
        assert!(matches!(statement.kind, MySqlStatementKind::Update { .. }));
        assert_eq!(statement.target, Some("items".to_string()));
    }

    #[test]
    fn rejects_unverifiable_version_comment() {
        assert!(classify_mysql_statement("/*! SET sql_mode='ANSI_QUOTES' */ SELECT 1").is_err());
    }

    #[test]
    fn classifies_leading_version_comment_statement() {
        assert!(matches!(
            classify_mysql_statement("/*!80000 SELECT 1 */"),
            Ok(MySqlStatement {
                kind: MySqlStatementKind::Select,
                ..
            })
        ));
    }

    #[test]
    fn preserves_utf8_unquoted_and_backtick_targets() {
        let cases = [
            ("UPDATE café SET value = 1", None, "café"),
            ("UPDATE 1é SET value = 1", None, "1é"),
            ("UPDATE 1abc SET value = 1", None, "1abc"),
            ("UPDATE 1_foo SET value = 1", None, "1_foo"),
            ("UPDATE 1$foo SET value = 1", None, "1$foo"),
            ("UPDATE 1$é SET value = 1", None, "1$é"),
            (
                "UPDATE 1abc.éléments SET value = 1",
                Some("1abc"),
                "éléments",
            ),
            ("UPDATE 1$foo.café SET value = 1", Some("1$foo"), "café"),
            ("UPDATE 1_foo.items SET value = 1", Some("1_foo"), "items"),
            (
                "UPDATE café.éléments SET value = 1",
                Some("café"),
                "éléments",
            ),
            ("UPDATE $items SET value = 1", None, "$items"),
            (
                r"UPDATE `café`.`éléments` SET value = 1",
                Some("café"),
                "éléments",
            ),
        ];

        for (sql, expected_database, expected_target) in cases {
            let statement = classify_mysql_statement(sql).expect(sql);
            assert_eq!(
                statement.target_database.as_deref(),
                expected_database,
                "{sql}"
            );
            assert_eq!(statement.target(), Some(expected_target), "{sql}");
        }

        assert!(classify_mysql_statement("UPDATE 123 SET value = 1").is_err());
    }

    #[test]
    fn rejects_mysql_numeric_literals_as_mutation_targets() {
        for sql in [
            "UPDATE 1e3 SET value = 1",
            "UPDATE 1e+3 SET value = 1",
            "UPDATE 1e-3 SET value = 1",
            "UPDATE 0x01AF SET value = 1",
            "UPDATE 0b01 SET value = 1",
        ] {
            assert!(classify_mysql_statement(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn preserves_utf8_targets_around_comments_and_executable_comments() {
        for sql in [
            "UPDATE /* ignored café */ café SET value = 1",
            "UPDATE café -- ignored comment\n SET value = 1",
            "/*!80000 UPDATE café SET value = 1 */",
            "CREATE TABLE café (id INT) /*!40100 DEFAULT CHARSET=utf8mb4 */",
            "DROP TABLE café /*!80000 RESTRICT */",
        ] {
            let statement = classify_mysql_statement(sql).expect(sql);
            assert_eq!(statement.target(), Some("café"), "{sql}");
        }

        for sql in [
            "UPDATE café--x\n SET value = 1",
            "UPDATE 1e+foo SET value = 1",
        ] {
            assert!(classify_mysql_statement(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn rejects_non_bmp_backtick_targets_fail_closed() {
        assert!(classify_mysql_statement("UPDATE `caf😀` SET value = 1").is_err());
        assert!(classify_mysql_statement("UPDATE caf😀 SET value = 1").is_err());
    }

    #[test]
    fn classifies_documented_mysql_ddl_forms() {
        let cases = [
            (
                "RENAME TABLE app.items TO app.archived_items",
                MySqlStatementKind::RenameTable,
                "items",
                Some("app"),
            ),
            (
                "RENAME TABLE items TO app.archived_items",
                MySqlStatementKind::RenameTable,
                "items",
                Some("app"),
            ),
            (
                "ALTER TABLE app.items RENAME TO app.archived_items",
                MySqlStatementKind::AlterTable,
                "items",
                Some("app"),
            ),
            (
                "ALTER TABLE app.items RENAME AS app.archived_items",
                MySqlStatementKind::AlterTable,
                "items",
                Some("app"),
            ),
            (
                "ALTER TABLE app.items RENAME COLUMN old_name TO new_name",
                MySqlStatementKind::AlterTable,
                "items",
                Some("app"),
            ),
            (
                "ALTER TABLE app.items RENAME INDEX old_index TO new_index",
                MySqlStatementKind::AlterTable,
                "items",
                Some("app"),
            ),
            (
                "CREATE OR REPLACE VIEW app.item_view AS SELECT id FROM app.items",
                MySqlStatementKind::CreateView,
                "item_view",
                Some("app"),
            ),
            (
                "CREATE OR REPLACE ALGORITHM=MERGE DEFINER=CURRENT_USER SQL SECURITY INVOKER VIEW app.item_view AS SELECT id FROM app.items",
                MySqlStatementKind::CreateView,
                "item_view",
                Some("app"),
            ),
            (
                "ALTER VIEW app.item_view AS SELECT id FROM app.items",
                MySqlStatementKind::AlterView,
                "item_view",
                Some("app"),
            ),
            (
                "ALTER ALGORITHM=MERGE DEFINER=CURRENT_USER SQL SECURITY INVOKER VIEW app.item_view AS SELECT id FROM app.items",
                MySqlStatementKind::AlterView,
                "item_view",
                Some("app"),
            ),
            (
                "CREATE FULLTEXT INDEX item_text ON app.items (body)",
                MySqlStatementKind::CreateIndex,
                "item_text",
                Some("app"),
            ),
        ];

        for (sql, expected_kind, expected_target, expected_database) in cases {
            let statement = classify_mysql_statement(sql).expect(sql);
            assert_eq!(statement.kind, expected_kind, "{sql}");
            assert_eq!(statement.target.as_deref(), Some(expected_target), "{sql}");
            assert_eq!(
                statement.target_database.as_deref(),
                expected_database,
                "{sql}"
            );
        }
    }

    #[test]
    fn accepts_only_trailing_ddl_version_comments() {
        let statement = classify_mysql_statement(
            "CREATE TABLE items (id INT) /*!40100 DEFAULT CHARSET=utf8mb4 */",
        )
        .expect("trailing DDL version comment");
        assert_eq!(
            statement.kind,
            MySqlStatementKind::CreateTable { temporary: false }
        );
        assert!(
            classify_mysql_statement("CREATE TABLE items (id INT) /* ordinary comment */").is_ok()
        );
        assert!(
            classify_mysql_statement(
                "CREATE TABLE items (id INT) /*!401 DEFAULT DROP TABLE other_items */"
            )
            .is_ok()
        );
        assert!(
            classify_mysql_statement(
                "CREATE TABLE items (id INT) /*!080400 DEFAULT CHARSET=utf8mb4 */"
            )
            .is_ok()
        );
        assert!(classify_mysql_statement("DROP TABLE items /*!80000 RESTRICT */").is_ok());
        assert!(classify_mysql_statement("DROP TABLE items /*!80000 CASCADE */").is_ok());

        for sql in [
            "CREATE TABLE items (id INT) /*!40100 DEFAULT CHARSET=utf8mb4 */ SELECT 1",
            "CREATE TABLE items (id INT) /*!40100 SET sql_mode='ANSI_QUOTES' */",
            "CREATE TABLE items (id INT) /*!80000 DEFAULT CHARSET=utf8mb4 DROP TABLE other_items */",
            "DROP TABLE items /*!80000 , other_items */",
            "CREATE TABLE items (id INT) /*!8000011 DEFAULT CHARSET=utf8mb4 */",
            "CREATE TABLE items (id INT) /*!80000DEFAULT CHARSET=utf8mb4 */",
            "CREATE TABLE items (id INT) /*!080000DEFAULT CHARSET=utf8mb4 */",
            "CREATE TABLE items (id INT) /*! 80000 DEFAULT CHARSET=utf8mb4 */",
            "CREATE TABLE items (id INT) /*!40100 */",
            "CREATE TABLE items (id INT) /*! DEFAULT CHARSET=utf8mb4 */",
            "CREATE TABLE items (id INT) /*!40100 DEFAULT CHARSET=utf8mb4",
            "SELECT 1 /*!40101 + 1 */",
        ] {
            assert!(classify_mysql_statement(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn rejects_unsupported_or_ambiguous_ddl_forms() {
        for sql in [
            "CREATE DATABASE app",
            "ALTER DATABASE app CHARACTER SET utf8mb4",
            "ALTER TABLE",
            "ALTER TABLE PARTITION BY HASH(id)",
            "ALTER TABLE ORDER BY value",
            "RENAME DATABASE app TO archive",
            "RENAME TABLE old_items TO archived_items, other_items TO other_archive",
            "RENAME TABLE app.items TO other.archived_items",
            "ALTER TABLE app.items RENAME TO other.archived_items",
            "ALTER TABLE app.items RENAME AS other.archived_items",
            "CREATE OR REPLACE TABLE items (id INT)",
            "CREATE OR REPLACE INDEX item_index ON items (id)",
            "CREATE SPATIAL INDEX item_location ON items (location)",
        ] {
            assert!(classify_mysql_statement(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn rejects_executable_version_comment_clause() {
        assert!(classify_mysql_statement("SELECT 1 /*!80000 INTO OUTFILE '/tmp/x' */").is_err());
        assert!(classify_mysql_statement("/*!80000 SELECT 1 */ SELECT 2").is_err());
        assert!(classify_mysql_statement("/*!80000 SELECT 1; DROP TABLE items */").is_err());
    }

    #[test]
    fn accepts_sql_calc_found_rows_in_an_executable_select_modifier_comment() {
        assert!(
            classify_mysql_statement(
                "SELECT /*!80000 SQL_CALC_FOUND_ROWS */ first_key FROM items WHERE FALSE"
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_multiple_drop_targets_and_ambiguous_ddl_quotes() {
        assert!(classify_mysql_statement("DROP TABLE app.keep, other.drop_me").is_err());
        assert!(classify_mysql_statement("DROP VIEW app.keep, other.drop_me").is_err());
        assert!(classify_mysql_statement("CREATE TABLE \"items\" (id INT)").is_err());
    }

    #[test]
    fn rejects_multiple_table_update_and_delete_statements() {
        for sql in [
            "UPDATE items, prices SET items.price = prices.price WHERE items.id = prices.id",
            "UPDATE items JOIN prices ON items.id = prices.id SET items.price = prices.price",
            "DELETE items, prices FROM items JOIN prices ON items.id = prices.id",
            "DELETE FROM items, prices USING items JOIN prices ON items.id = prices.id",
            "DELETE items FROM items JOIN prices ON items.id = prices.id",
            "DELETE FROM items USING items JOIN prices ON items.id = prices.id",
            "DELETE FROM items JOIN prices ON items.id = prices.id",
        ] {
            let error = classify_mysql_statement(sql).unwrap_err();
            assert!(error.0.contains("multiple-table"), "{sql}: {error}");
        }
    }

    #[test]
    fn accepts_single_table_delete_clause_boundaries_and_nested_commas() {
        for sql in [
            "DELETE FROM items ORDER BY created_at, id LIMIT 10",
            "DELETE FROM items WHERE id IN (SELECT item_id FROM prices, currencies)",
            "DELETE FROM items LIMIT 10",
            "DELETE FROM `items,archive` ORDER BY id LIMIT 10",
        ] {
            let statement = classify_mysql_statement(sql).expect(sql);
            assert_eq!(
                statement.kind,
                MySqlStatementKind::Delete {
                    has_where: sql.contains("WHERE")
                },
                "{sql}"
            );
        }
    }

    #[test]
    fn rejects_top_level_into_clauses_before_execution() {
        for sql in [
            "SELECT id INTO OUTFILE '/tmp/result' FROM items",
            "SELECT id INTO DUMPFILE '/tmp/result' FROM items",
            "TABLE items INTO OUTFILE '/tmp/result'",
            "TABLE items INTO @picked",
            "WITH rows AS (SELECT 1) SELECT * INTO OUTFILE '/tmp/result' FROM rows",
        ] {
            let error = classify_mysql_multi_statement(sql, Some("app")).unwrap_err();
            assert!(error.contains("SELECT INTO clauses"), "{sql}: {error}");
        }
        assert!(
            classify_mysql_multi_statement(
                "WITH rows AS (SELECT 'INTO OUTFILE') SELECT * FROM rows",
                Some("app")
            )
            .is_ok()
        );
        assert!(
            classify_mysql_multi_statement(
                "SELECT id INTO @value FROM items; SELECT @value",
                Some("app")
            )
            .is_ok()
        );
    }

    #[test]
    fn allows_single_table_mutations_with_nested_table_references() {
        for sql in [
            "UPDATE items SET value = (SELECT MAX(value) FROM prices JOIN currencies ON prices.currency_id = currencies.id) WHERE id = 1",
            "DELETE FROM items WHERE id IN (SELECT item_id FROM prices JOIN currencies ON prices.currency_id = currencies.id)",
            "UPDATE items PARTITION (p0, p1) SET value = 1 WHERE id = 1",
            "UPDATE items USE INDEX FOR JOIN (idx_items) SET value = 1 WHERE id = 1",
            "DELETE FROM items PARTITION (p0, p1) WHERE id = 1",
        ] {
            assert!(classify_mysql_statement(sql).is_ok(), "{sql}");
        }
    }

    #[test]
    fn rejects_executable_inline_control_statement() {
        assert!(
            classify_mysql_statement("SELECT 1 /*!80000 SET sql_mode='ANSI_QUOTES' */").is_err()
        );
    }

    #[test]
    fn rejects_mysql_client_commands_at_line_start() {
        for sql in [
            "DELIMITER //\nSELECT 1//",
            "charset utf8mb4\nSELECT 1",
            "source ./script.sql",
            "system echo unsafe",
            "\\C /tmp/other.sock\nSELECT 1",
            "SELECT 1\nsource ./script.sql",
        ] {
            assert!(statement_contains_unsupported_mysql_control(sql), "{sql}");
        }
        assert!(!statement_contains_unsupported_mysql_control(
            "SELECT 'source ./script.sql\\n'"
        ));
    }

    #[test]
    fn rejects_mysql_client_commands_outside_literals_and_comments() {
        for sql in [
            r"SELECT 1 \G",
            r"SELECT 1\!",
            r"SELECT 1 \.",
            r"SELECT 1 \C utf8mb4",
        ] {
            assert!(statement_contains_unsupported_mysql_control(sql), "{sql}");
        }
        for sql in [
            r"SELECT '\G'",
            r"SELECT `\!`",
            r"SELECT 1 /* \.; \C */",
            "SELECT 1 -- \\!\n",
        ] {
            assert!(!statement_contains_unsupported_mysql_control(sql), "{sql}");
        }
    }

    #[test]
    fn keeps_index_confirmation_name_separate_from_ddl_database_target() {
        let statement = classify_mysql_statement("DROP INDEX ix ON app.items").unwrap();
        assert_eq!(statement.target, Some("ix".to_string()));
        assert_eq!(statement.target_database, Some("app".to_string()));
        let statement = classify_mysql_statement("DROP INDEX IF EXISTS ix ON app.items").unwrap();
        assert_eq!(statement.target, Some("ix".to_string()));
    }

    #[test]
    fn preserves_confirmation_target_case_for_unquoted_and_quoted_names() {
        let statement = classify_mysql_statement("DROP TABLE SalesOrder").unwrap();
        assert_eq!(statement.target, Some("SalesOrder".to_string()));

        let statement = classify_mysql_statement("DROP TABLE SalesDb.SalesOrder").unwrap();
        assert_eq!(statement.target, Some("SalesOrder".to_string()));
        assert_eq!(statement.target_database, Some("SalesDb".to_string()));

        let statement = classify_mysql_statement("DROP TABLE `SalesOrder`").unwrap();
        assert_eq!(statement.target, Some("SalesOrder".to_string()));
    }
}

mod validation {
    use super::super::*;

    #[test]
    fn classifies_mysql_multi_statement_for_execution() {
        let statements =
            classify_mysql_multi_statement("UPDATE items SET value = 1; SELECT 2", Some("app"))
                .expect("valid MySQL statements");

        assert!(matches!(
            statements[0].kind,
            MySqlStatementKind::Update { has_where: false }
        ));
        assert_eq!(statements[1].sql, "SELECT 2");
    }

    #[test]
    fn accepts_comments_hints_and_version_comments() {
        assert!(
            classify_mysql_multi_statement(
                "SELECT /*+ MAX_EXECUTION_TIME(1000) */ 1 # trailing\n",
                Some("app")
            )
            .is_ok()
        );
        assert!(classify_mysql_multi_statement("/*!80000 SELECT 1 */", Some("app")).is_ok());
    }

    #[test]
    fn distinguishes_persistent_mysql_schema_changes_from_temporary_tables() {
        assert!(mysql_statement_is_persistent_schema_change(
            &MySqlStatementKind::CreateTable { temporary: false }
        ));
        assert!(!mysql_statement_is_persistent_schema_change(
            &MySqlStatementKind::CreateTable { temporary: true }
        ));
        assert!(!mysql_statement_is_persistent_schema_change(
            &MySqlStatementKind::DropTable { temporary: true }
        ));
    }

    #[test]
    fn rejects_invalid_mysql_scripts_before_execution() {
        for sql in [
            "SELECT 'unfinished",
            "SELECT 1 /* unfinished",
            "MERGE INTO items USING source ON items.id = source.id",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_err(),
                "{sql}"
            );
        }
    }

    #[test]
    fn accepts_replace_for_execution() {
        let statements = classify_mysql_multi_statement(
            "REPLACE INTO items (id, value) VALUES (1, 'new')",
            Some("app"),
        )
        .expect("REPLACE should use the regular MySQL execution path");

        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].kind(), &MySqlStatementKind::Replace);
        assert_eq!(statements[0].target(), Some("items"));
    }

    #[test]
    fn preserves_mysql_split_errors_for_callers() {
        assert!(classify_mysql_multi_statement("SELECT 'unfinished", Some("app")).is_err());
        assert!(classify_mysql_multi_statement("SELECT 1 /* unfinished", Some("app")).is_err());
    }

    #[test]
    fn rejects_unsupported_controls_and_statements_before_execution() {
        for sql in [
            "USE app",
            "SET sql_mode = 'ANSI_QUOTES'",
            "LOCK TABLES items READ",
            "UNLOCK TABLES",
            "CALL do_work()",
            "LOAD DATA INFILE 'x' INTO TABLE items",
            "/*! SET sql_mode='ANSI_QUOTES' */ SELECT 1",
            "SELECT 1 /*!40101 + 1 */",
            "DELIMITER //\nSELECT 1//",
            "charset utf8mb4\nSELECT 1",
            "source ./script.sql",
            "system echo unsafe",
            "\\C /tmp/other.sock\nSELECT 1",
            "SELECT 1\n\\! echo unsafe",
            "SELECT 1\nsystem echo unsafe",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_err(),
                "{sql}"
            );
        }
        for sql in [
            "SELECT 'source ./script.sql\\n'",
            "SELECT /* source ./script.sql */ 1",
            "SELECT `system echo unsafe`",
            "UPDATE items\nSET value = 1 WHERE id = 1",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_ok(),
                "{sql}"
            );
        }
    }

    #[test]
    fn rejects_ambiguous_mysql_mutation_targets() {
        for sql in [
            "ALTER TABLE",
            "ALTER TABLE ADD COLUMN value INT",
            "ALTER TABLE ALTER COLUMN value DROP DEFAULT",
            "ALTER TABLE DROP COLUMN value",
            "ALTER TABLE MODIFY COLUMN value INT",
            "ALTER TABLE RENAME TO archived_items",
            "ALTER TABLE TRUNCATE PARTITION p0",
            "ALTER TABLE LOCK=EXCLUSIVE",
            "ALTER TABLE PARTITION BY HASH(id)",
            "ALTER TABLE FORCE",
            "ALTER TABLE ORDER BY value",
            "ALTER TABLE SECONDARY_LOAD",
            "ALTER TABLE SECONDARY_LOAD PARTITION (p0)",
            "ALTER TABLE SECONDARY_UNLOAD",
            "ALTER TABLE SECONDARY_UNLOAD PARTITION (p0)",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_err(),
                "{sql}"
            );
        }
    }

    #[test]
    fn ddl_rejects_a_different_database() {
        for sql in [
            "ALTER TABLE other.items ADD COLUMN value INT",
            "ALTER TABLE app.items RENAME TO other.archived_items",
            "ALTER TABLE items RENAME TO other.archived_items",
            "ALTER TABLE app.items RENAME AS other.archived_items",
            "RENAME TABLE app.items TO other.archived_items",
            "DROP TABLE app.keep, other.drop_me",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_err(),
                "{sql}"
            );
        }
        for sql in [
            "ALTER TABLE app.items ADD COLUMN value INT",
            "RENAME TABLE items TO app.archived_items",
            "ALTER TABLE app.items RENAME TO app.archived_items",
            "ALTER TABLE app.items RENAME AS app.archived_items",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_ok(),
                "{sql}"
            );
        }
        assert!(classify_mysql_multi_statement("CREATE TABLE items (id INT)", None).is_err());
    }

    #[test]
    fn qualified_mysql_mutations_require_exact_selected_database() {
        for sql in [
            "INSERT INTO other.items VALUES (1)",
            "UPDATE other.items SET value = 1",
            "DELETE FROM other.items WHERE id = 1",
            "INSERT INTO APP.items VALUES (1)",
            "UPDATE APP.items SET value = 1",
            "DELETE FROM APP.items WHERE id = 1",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_err(),
                "{sql}"
            );
        }

        for sql in [
            "INSERT INTO items VALUES (1)",
            "UPDATE items SET value = 1",
            "DELETE FROM items WHERE id = 1",
            "INSERT INTO app.items VALUES (1)",
            "UPDATE app.items SET value = 1",
            "DELETE FROM app.items WHERE id = 1",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_ok(),
                "{sql}"
            );
        }
    }

    #[test]
    fn qualified_mysql_mutations_follow_lower_case_table_names() {
        for lower_case_table_names in [1, 2] {
            for sql in [
                "INSERT INTO APP.items VALUES (1)",
                "UPDATE APP.items SET value = 1",
                "DELETE FROM APP.items WHERE id = 1",
            ] {
                let statements = classify_mysql_multi_statement_with_lower_case_table_names(
                    sql,
                    Some("app"),
                    lower_case_table_names,
                )
                .unwrap_or_else(|error| panic!("{sql}: {error}"));
                assert_eq!(statements[0].target_database.as_deref(), Some("APP"));
                assert_eq!(statements[0].target(), Some("items"));
            }
        }

        assert!(
            classify_mysql_multi_statement_with_lower_case_table_names(
                "UPDATE APP.items SET value = 1",
                Some("app"),
                0,
            )
            .is_err()
        );
        assert!(
            classify_mysql_multi_statement_with_lower_case_table_names(
                "UPDATE other.items SET value = 1",
                Some("app"),
                1,
            )
            .is_err()
        );
    }

    #[test]
    fn qualified_utf8_mysql_mutations_follow_selected_database_case_rules() {
        let exact = classify_mysql_multi_statement_with_lower_case_table_names(
            "UPDATE äpp.éléments SET value = 1",
            Some("äpp"),
            0,
        )
        .unwrap();
        assert_eq!(exact[0].target_database.as_deref(), Some("äpp"));
        assert_eq!(exact[0].target(), Some("éléments"));

        for lower_case_table_names in [1, 2] {
            let statements = classify_mysql_multi_statement_with_lower_case_table_names(
                "UPDATE ÄPP.éléments SET value = 1",
                Some("äpp"),
                lower_case_table_names,
            )
            .unwrap();
            assert_eq!(statements[0].target_database.as_deref(), Some("ÄPP"));
            assert_eq!(statements[0].target(), Some("éléments"));
        }

        assert!(
            classify_mysql_multi_statement_with_lower_case_table_names(
                "UPDATE ÄPP.éléments SET value = 1",
                Some("äpp"),
                0,
            )
            .is_err()
        );
    }

    #[test]
    fn temporary_mysql_table_keys_follow_unicode_table_case_rules() {
        assert!(
            classify_mysql_multi_statement_with_lower_case_table_names(
                "CREATE TEMPORARY TABLE café (id INT); DROP TEMPORARY TABLE CAFÉ",
                Some("app"),
                0,
            )
            .is_err()
        );

        for lower_case_table_names in [1, 2] {
            assert!(
                classify_mysql_multi_statement_with_lower_case_table_names(
                    "CREATE TEMPORARY TABLE café (id INT); DROP TEMPORARY TABLE CAFÉ",
                    Some("app"),
                    lower_case_table_names,
                )
                .is_ok()
            );
        }

        assert!(
            classify_mysql_multi_statement_with_lower_case_table_names(
                "CREATE TEMPORARY TABLE items (id INT); CREATE TEMPORARY TABLE ITEMS (id INT); DROP TEMPORARY TABLE items; DROP TEMPORARY TABLE ITEMS",
                Some("app"),
                0,
            )
            .is_ok()
        );
    }

    #[test]
    fn qualified_mysql_mutations_match_unicode_database_names_without_rewriting_targets() {
        for lower_case_table_names in [1, 2] {
            let statements = classify_mysql_multi_statement_with_lower_case_table_names(
                "UPDATE `ÄPP`.items SET value = 1",
                Some("äpp"),
                lower_case_table_names,
            )
            .unwrap();

            assert_eq!(statements[0].target_database.as_deref(), Some("ÄPP"));
            assert_eq!(statements[0].target(), Some("items"));
        }

        assert!(
            classify_mysql_multi_statement_with_lower_case_table_names(
                "UPDATE `ÄPP`.items SET value = 1",
                Some("äpp"),
                0,
            )
            .is_err()
        );
    }

    #[test]
    fn temporary_mysql_table_keys_follow_unicode_database_case_rules() {
        for lower_case_table_names in [1, 2] {
            assert!(
                classify_mysql_multi_statement_with_lower_case_table_names(
                    "CREATE TEMPORARY TABLE temp_items (id INT); DROP TEMPORARY TABLE `ÄPP`.temp_items",
                    Some("äpp"),
                    lower_case_table_names,
                )
                .is_ok()
            );
        }

        assert!(
            classify_mysql_multi_statement_with_lower_case_table_names(
                "CREATE TEMPORARY TABLE temp_items (id INT); DROP TEMPORARY TABLE `ÄPP`.temp_items",
                Some("äpp"),
                0,
            )
            .is_err()
        );
    }

    #[test]
    fn rename_rejects_database_names_that_differ_only_by_case() {
        for sql in [
            "ALTER TABLE APP.items ADD COLUMN value INT",
            "ALTER TABLE app.items RENAME TO APP.archived_items",
            "RENAME TABLE app.items TO APP.archived_items",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_err(),
                "{sql}"
            );
        }
    }
}

mod transaction {
    use super::super::*;

    #[test]
    fn unicode_savepoints_are_case_insensitive_in_transactions() {
        assert!(
            classify_mysql_multi_statement(
                "START TRANSACTION; SAVEPOINT café; ROLLBACK TO SAVEPOINT CAFÉ; COMMIT",
                Some("app"),
            )
            .is_ok()
        );
        assert!(
            classify_mysql_multi_statement(
                "START TRANSACTION; SAVEPOINT ς; ROLLBACK TO SAVEPOINT σ; COMMIT",
                Some("app"),
            )
            .is_ok()
        );
    }

    #[test]
    fn transaction_modifiers_are_rejected() {
        for sql in [
            "START TRANSACTION READ ONLY",
            "COMMIT AND CHAIN",
            "ROLLBACK AND NO CHAIN",
            "ROLLBACK TO SAVEPOINT named extra",
            "RELEASE SAVEPOINT named extra",
            "BEGIN",
            "BEGIN; UPDATE items SET value = 1",
            "SAVEPOINT named",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_err(),
                "{sql}"
            );
        }
        for sql in [
            "COMMIT",
            "ROLLBACK",
            "BEGIN; COMMIT",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; ROLLBACK",
            "START TRANSACTION; ROLLBACK",
            "BEGIN; SAVEPOINT named; ROLLBACK TO named; RELEASE SAVEPOINT named; COMMIT",
            "CREATE TEMPORARY TABLE temp_items (id INT); INSERT INTO temp_items VALUES (1); SELECT * FROM temp_items",
            "CREATE TEMPORARY TABLE temp_items (id INT); DROP TEMPORARY TABLE app.temp_items",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_ok(),
                "{sql}"
            );
        }
    }

    #[test]
    fn rejects_persistent_ddl_inside_an_explicit_transaction() {
        for sql in [
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; CREATE TABLE new_items (id INT); ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; ALTER TABLE items ADD COLUMN extra INT; ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; DROP TABLE items; ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; TRUNCATE TABLE items; ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; CREATE VIEW item_view AS SELECT 1; ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; DROP VIEW item_view; ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; CREATE INDEX item_index ON items (value); ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; DROP INDEX item_index ON items; ROLLBACK",
        ] {
            let error = classify_mysql_multi_statement(sql, Some("app")).unwrap_err();
            assert!(error.contains("implicit commit"), "{sql}: {error}");
        }
    }
}

mod explain_parsing {
    use super::super::*;

    #[test]
    fn recognizes_generated_tree_explain_queries() {
        assert_eq!(
            mysql_tree_explain_query_kind("EXPLAIN FORMAT=TREE UPDATE items SET value = 1"),
            Some(false)
        );
        assert_eq!(
            mysql_tree_explain_query_kind("EXPLAIN ANALYZE FORMAT=TREE TABLE items"),
            Some(true)
        );
        assert_eq!(
            mysql_tree_explain_query_kind("EXPLAIN ANALYZE FORMAT=TREE DELETE FROM items"),
            None
        );
    }

    #[test]
    fn allows_replace_for_tree_explain_but_not_explain_analyze() {
        for query in [
            "REPLACE INTO items (id, value) VALUES (1, 'new')",
            "REPLACE items (id, value) VALUES (1, 'new')",
        ] {
            assert_eq!(mysql_explain_rejection_message(query), None, "{query}");
            assert_eq!(
                mysql_tree_explain_query_kind(&format!("EXPLAIN FORMAT=TREE {query}")),
                Some(false),
                "{query}"
            );
            assert_eq!(
                mysql_tree_explain_query_kind(&format!("EXPLAIN ANALYZE FORMAT=TREE {query}")),
                None,
                "{query}"
            );
        }
    }

    #[test]
    fn rejects_replace_without_a_classifiable_target_for_explain() {
        for query in ["REPLACE", "REPLACE INTO"] {
            assert_eq!(
                mysql_explain_rejection_message(query),
                Some(
                    "MySQL EXPLAIN supports SELECT, TABLE, INSERT, REPLACE, UPDATE, or DELETE statements"
                ),
                "{query}"
            );
        }
    }

    #[test]
    fn recognizes_tree_explain_queries_with_whitespace_around_equals() {
        assert_eq!(
            mysql_tree_explain_query_kind("EXPLAIN FORMAT = TREE UPDATE items SET value = 1"),
            Some(false)
        );
        assert_eq!(
            mysql_tree_explain_query_kind("EXPLAIN ANALYZE FORMAT = TREE TABLE items"),
            Some(true)
        );
    }

    #[test]
    fn tree_explain_prefix_keeps_keyword_boundaries_and_ignores_non_sql_text() {
        for sql in [
            "EXPLAIN FORMAT = JSON SELECT 1",
            "EXPLAIN FORMAT = TREEish SELECT 1",
            "EXPLAINFORMAT = TREE SELECT 1",
            "/* EXPLAIN FORMAT = TREE */ SELECT 1",
            "SELECT 'EXPLAIN FORMAT = TREE SELECT 1'",
        ] {
            assert_eq!(mysql_tree_explain_query_kind(sql), None, "{sql}");
        }
    }
}
