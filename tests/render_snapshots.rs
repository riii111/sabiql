mod harness;

use harness::fixtures;
use harness::{
    create_test_state, create_test_terminal, render_and_get_buffer, render_to_string, test_instant,
};

use std::sync::Arc;
use std::time::Duration;

use sabiql::app::connection_error::{ConnectionErrorInfo, ConnectionErrorKind};
use sabiql::app::connection_setup_state::ConnectionField;
use sabiql::app::er_state::ErStatus;
use sabiql::app::focused_pane::FocusedPane;
use sabiql::app::input_mode::InputMode;
use sabiql::app::sql_modal_context::{AdhocSuccessSnapshot, SqlModalStatus};
use sabiql::app::write_guardrails::{
    ColumnDiff, GuardrailDecision, RiskLevel, TargetSummary, WriteOperation, WritePreview,
};
use sabiql::domain::{CommandTag, QuerySource};

mod initial_state {
    use super::*;

    #[test]
    fn initial_state_no_metadata() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn explorer_shows_not_connected_when_no_active_connection() {
        let mut state = create_test_state();
        state.session.active_connection_name = None;
        let mut terminal = create_test_terminal();

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }
}

mod table_explorer {
    use super::*;

    #[test]
    fn table_selection_with_preview() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn focus_on_result_pane() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn focus_mode_fullscreen_result() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focus_mode = true;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn error_message_in_footer() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        state.messages.last_error = Some("Connection failed: timeout".to_string());
        state.messages.expires_at = Some(now + Duration::from_secs(10));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn empty_query_result() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state
            .query
            .set_current_result(Arc::new(fixtures::empty_query_result(now)));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }
}

mod overlays {
    use super::*;

    #[test]
    fn sql_modal_with_completion() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.content = "SELECT * FROM us".to_string();
        state.sql_modal.cursor = 16;
        state.sql_modal.set_status(SqlModalStatus::Editing);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn sql_modal_cursor_at_head() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.content = "SELECT 1".to_string();
        state.sql_modal.cursor = 0;
        state.sql_modal.set_status(SqlModalStatus::Editing);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn sql_modal_cursor_at_middle() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.content = "SELECT 1".to_string();
        state.sql_modal.cursor = 4;
        state.sql_modal.set_status(SqlModalStatus::Editing);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn sql_modal_cursor_at_tail() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.content = "SELECT 1".to_string();
        state.sql_modal.cursor = 8;
        state.sql_modal.set_status(SqlModalStatus::Editing);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn sql_modal_success_select() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.content = "SELECT * FROM users".to_string();
        state.sql_modal.mark_adhoc_success(AdhocSuccessSnapshot {
            command_tag: None,
            row_count: 2,
            execution_time_ms: 15,
        });
        state
            .query
            .set_current_result(Arc::new(sabiql::domain::QueryResult {
                query: "SELECT * FROM users".to_string(),
                columns: vec!["id".to_string(), "name".to_string()],
                rows: vec![
                    vec!["1".to_string(), "Alice".to_string()],
                    vec!["2".to_string(), "Bob".to_string()],
                ],
                row_count: 2,
                execution_time_ms: 15,
                executed_at: now,
                source: QuerySource::Adhoc,
                error: None,
                command_tag: None,
            }));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn sql_modal_success_dml_with_command_tag() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.content = "DELETE FROM users WHERE id = 1".to_string();
        state.sql_modal.mark_adhoc_success(AdhocSuccessSnapshot {
            command_tag: Some(CommandTag::Delete(3)),
            row_count: 3,
            execution_time_ms: 12,
        });
        state
            .query
            .set_current_result(Arc::new(sabiql::domain::QueryResult {
                query: "DELETE FROM users WHERE id = 1".to_string(),
                columns: vec![],
                rows: vec![],
                row_count: 3,
                execution_time_ms: 12,
                executed_at: now,
                source: QuerySource::Adhoc,
                error: None,
                command_tag: Some(CommandTag::Delete(3)),
            }));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn sql_modal_success_ddl_create_table() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.content = "CREATE TABLE backup AS SELECT * FROM users".to_string();
        state.sql_modal.mark_adhoc_success(AdhocSuccessSnapshot {
            command_tag: Some(CommandTag::Create("TABLE".to_string())),
            row_count: 0,
            execution_time_ms: 45,
        });
        state
            .query
            .set_current_result(Arc::new(sabiql::domain::QueryResult {
                query: "CREATE TABLE backup AS SELECT * FROM users".to_string(),
                columns: vec![],
                rows: vec![],
                row_count: 0,
                execution_time_ms: 45,
                executed_at: now,
                source: QuerySource::Adhoc,
                error: None,
                command_tag: Some(CommandTag::Create("TABLE".to_string())),
            }));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn sql_modal_error_with_message() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.content = "SELECT * FORM users".to_string();
        state.sql_modal.mark_adhoc_error("ERROR:  syntax error at or near \"FORM\"\nLINE 1: SELECT * FORM users\n                 ^".to_string());

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn help_overlay() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.modal.set_mode(InputMode::Help);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn command_palette_overlay() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.modal.set_mode(InputMode::CommandPalette);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn table_picker_overlay() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.modal.set_mode(InputMode::TablePicker);
        state.ui.filter_input = "user".to_string();

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn command_line_input() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.modal.set_mode(InputMode::CommandLine);
        state.command_line_input = "sql".to_string();

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn query_history_picker_with_entries() {
        use sabiql::domain::ConnectionId;
        use sabiql::domain::query_history::QueryHistoryEntry;

        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::QueryHistoryPicker);
        state.query_history_picker.entries = vec![
            QueryHistoryEntry::new(
                "SELECT * FROM users WHERE id = 1".to_string(),
                "2026-03-13T10:00:00Z".to_string(),
                ConnectionId::from_string("test-conn"),
            ),
            QueryHistoryEntry::new(
                "INSERT INTO orders (user_id, total) VALUES (1, 100)".to_string(),
                "2026-03-13T11:00:00Z".to_string(),
                ConnectionId::from_string("test-conn"),
            ),
            QueryHistoryEntry::new(
                "SELECT count(*) FROM users".to_string(),
                "2026-03-13T12:00:00Z".to_string(),
                ConnectionId::from_string("test-conn"),
            ),
        ];

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn query_history_picker_empty() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::QueryHistoryPicker);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }
}

mod er_diagram {
    use super::*;

    #[test]
    fn er_waiting_progress() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));

        state.er_preparation.status = ErStatus::Waiting;
        state.er_preparation.total_tables = 3;
        state
            .er_preparation
            .pending_tables
            .insert("public.comments".to_string());
        state
            .er_preparation
            .fetching_tables
            .insert("public.posts".to_string());

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn er_table_picker_modal() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.modal.set_mode(InputMode::ErTablePicker);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn er_table_picker_filtered() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.modal.set_mode(InputMode::ErTablePicker);
        state.ui.er_filter_input = "user".to_string();

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn er_table_picker_single_select() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.modal.set_mode(InputMode::ErTablePicker);
        state
            .ui
            .er_selected_tables
            .insert("public.users".to_string());

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn er_table_picker_multi_select() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.modal.set_mode(InputMode::ErTablePicker);
        state
            .ui
            .er_selected_tables
            .insert("public.users".to_string());
        state
            .ui
            .er_selected_tables
            .insert("public.posts".to_string());

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn er_table_picker_all_selected() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.modal.set_mode(InputMode::ErTablePicker);
        state
            .ui
            .er_selected_tables
            .insert("public.users".to_string());
        state
            .ui
            .er_selected_tables
            .insert("public.posts".to_string());
        state
            .ui
            .er_selected_tables
            .insert("public.comments".to_string());

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }
}

mod connection_flow {
    use super::*;

    #[test]
    fn connection_setup_form() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::ConnectionSetup);
        state.connection_setup.database = "mydb".to_string();
        state.connection_setup.user = "postgres".to_string();

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_setup_cursor_at_head() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::ConnectionSetup);
        state.connection_setup.focused_field = ConnectionField::Host;
        state.connection_setup.host = "db.example.com".to_string();
        state.connection_setup.cursor_position = 0;
        state.connection_setup.viewport_offset = 0;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_setup_cursor_at_middle() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::ConnectionSetup);
        state.connection_setup.focused_field = ConnectionField::Host;
        state.connection_setup.host = "db.example.com".to_string();
        state.connection_setup.cursor_position = 7;
        state.connection_setup.viewport_offset = 0;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_setup_cursor_at_tail() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::ConnectionSetup);
        state.connection_setup.focused_field = ConnectionField::Host;
        state.connection_setup.host = "db.example.com".to_string();
        state.connection_setup.cursor_position = 14;
        state.connection_setup.viewport_offset = 0;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_setup_with_validation_errors() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::ConnectionSetup);
        state.connection_setup.host = String::new();
        state.connection_setup.database = String::new();
        state.connection_setup.user = String::new();
        state
            .connection_setup
            .validation_errors
            .insert(ConnectionField::Host, "Required".to_string());
        state
            .connection_setup
            .validation_errors
            .insert(ConnectionField::Database, "Required".to_string());
        state
            .connection_setup
            .validation_errors
            .insert(ConnectionField::User, "Required".to_string());

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_error_collapsed() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::ConnectionError);
        state
            .connection_error
            .set_error(ConnectionErrorInfo::with_kind(
                ConnectionErrorKind::HostUnreachable,
                "psql: error: could not translate host name \"db.example.com\" to address",
            ));
        state.connection_error.details_expanded = false;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_error_expanded() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::ConnectionError);
        state.connection_error.set_error(ConnectionErrorInfo::with_kind(
            ConnectionErrorKind::Timeout,
            "psql: error: connection to server at \"192.168.1.100\", port 5432 failed: timeout expired",
        ));
        state.connection_error.details_expanded = true;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_error_expanded_with_tabs() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::ConnectionError);
        state.connection_error.set_error(ConnectionErrorInfo::with_kind(
            ConnectionErrorKind::Unknown,
            "psql: error: connection to server at \"localhost\" (127.0.0.1), port 5433 failed: Connection refused\n\tIs the server running on that host and accepting TCP/IP connections?",
        ));
        state.connection_error.details_expanded = true;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn footer_shows_success_message() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.messages.last_success = Some("Reconnected!".to_string());

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }
}

mod connection_management {
    use super::*;
    use sabiql::domain::connection::{ConnectionId, ConnectionName, ConnectionProfile, SslMode};

    fn three_connections() -> (ConnectionId, Vec<ConnectionProfile>) {
        let active_id = ConnectionId::new();
        let profiles = vec![
            ConnectionProfile {
                id: active_id.clone(),
                name: ConnectionName::new("Production").unwrap(),
                host: "prod.example.com".to_string(),
                port: 5432,
                database: "prod_db".to_string(),
                username: "admin".to_string(),
                password: "secret".to_string(),
                ssl_mode: SslMode::Require,
            },
            ConnectionProfile {
                id: ConnectionId::new(),
                name: ConnectionName::new("Staging").unwrap(),
                host: "staging.example.com".to_string(),
                port: 5432,
                database: "staging_db".to_string(),
                username: "user".to_string(),
                password: "pass".to_string(),
                ssl_mode: SslMode::Prefer,
            },
            ConnectionProfile {
                id: ConnectionId::new(),
                name: ConnectionName::new("Local Dev").unwrap(),
                host: "localhost".to_string(),
                port: 5432,
                database: "dev_db".to_string(),
                username: "dev".to_string(),
                password: "dev".to_string(),
                ssl_mode: SslMode::Disable,
            },
        ];
        (active_id, profiles)
    }

    #[test]
    fn connection_selector_with_multiple_connections() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        let (active_id, connections) = three_connections();
        state.set_connections(connections);
        state.session.active_connection_id = Some(active_id);
        state.modal.set_mode(InputMode::ConnectionSelector);
        state.ui.set_connection_list_selection(Some(0));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_selector_with_service_entries() {
        use sabiql::domain::connection::ServiceEntry;

        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        let (active_id, connections) = three_connections();
        state.set_connections_and_services(
            connections,
            vec![
                ServiceEntry {
                    service_name: "dev-db".to_string(),
                    host: Some("localhost".to_string()),
                    dbname: Some("devdb".to_string()),
                    port: Some(5432),
                    user: Some("dev".to_string()),
                },
                ServiceEntry {
                    service_name: "prod-replica".to_string(),
                    host: Some("replica.example.com".to_string()),
                    dbname: Some("proddb".to_string()),
                    port: Some(5433),
                    user: None,
                },
            ],
        );
        state.session.active_connection_id = Some(active_id);
        state.modal.set_mode(InputMode::ConnectionSelector);
        state.ui.set_connection_list_selection(Some(0));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_selector_with_long_service_name() {
        use sabiql::domain::connection::ServiceEntry;

        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.set_service_entries(vec![
            ServiceEntry {
                service_name: "my-very-long-service-name-that-exceeds-normal-length".to_string(),
                host: Some("db.example.com".to_string()),
                dbname: Some("mydb".to_string()),
                port: Some(5432),
                user: None,
            },
            ServiceEntry {
                service_name: "short".to_string(),
                host: Some("localhost".to_string()),
                dbname: None,
                port: None,
                user: None,
            },
        ]);
        state.modal.set_mode(InputMode::ConnectionSelector);
        state.ui.set_connection_list_selection(Some(0));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_selector_with_active_service() {
        use sabiql::domain::connection::{ConnectionId, ServiceEntry};

        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.set_service_entries(vec![
            ServiceEntry {
                service_name: "dev-local".to_string(),
                host: Some("localhost".to_string()),
                dbname: Some("devdb".to_string()),
                port: Some(5432),
                user: Some("dev".to_string()),
            },
            ServiceEntry {
                service_name: "prod-replica".to_string(),
                host: Some("replica.example.com".to_string()),
                dbname: Some("proddb".to_string()),
                port: Some(5433),
                user: None,
            },
        ]);
        // Set active connection to the first service entry
        state.session.active_connection_id =
            Some(ConnectionId::from_string("service:dev-local".to_string()));
        state.modal.set_mode(InputMode::ConnectionSelector);
        state.ui.set_connection_list_selection(Some(0));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_selector_with_multibyte_service_name() {
        use sabiql::domain::connection::ServiceEntry;

        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.set_service_entries(vec![ServiceEntry {
            service_name: "本番データベース接続".to_string(),
            host: Some("db.example.com".to_string()),
            dbname: Some("mydb".to_string()),
            port: Some(5432),
            user: None,
        }]);
        state.modal.set_mode(InputMode::ConnectionSelector);
        state.ui.set_connection_list_selection(Some(0));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn confirm_dialog_delete_active_connection() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        let connection_id = ConnectionId::new();
        state.modal.set_mode(InputMode::ConfirmDialog);
        state.confirm_dialog.open(
            "Delete Connection",
            "Delete \"Production\"?\n\n\u{26A0} This is the active connection.\nYou will be disconnected.\n\nThis action cannot be undone.",
            sabiql::app::confirm_dialog_state::ConfirmIntent::DeleteConnection(connection_id),
        );

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn confirm_dialog_delete_inactive_connection() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        let target_id = ConnectionId::new();
        state.modal.set_mode(InputMode::ConfirmDialog);
        state.confirm_dialog.open(
            "Delete Connection",
            "Delete \"Staging\"?\n\nThis action cannot be undone.",
            sabiql::app::confirm_dialog_state::ConfirmIntent::DeleteConnection(target_id),
        );

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }
}

mod confirm_dialogs {
    use super::*;

    #[test]
    fn confirm_dialog() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::ConfirmDialog);
        state.confirm_dialog.open(
            "Confirm",
            "No connection configured.\nAre you sure you want to quit?",
            sabiql::app::confirm_dialog_state::ConfirmIntent::QuitNoConnection,
        );

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn confirm_dialog_update_preview() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state.modal.set_mode(InputMode::ConfirmDialog);
        state.confirm_dialog.open(
            "Confirm UPDATE: users",
            "email: \"bob@example.com\" -> \"new@example.com\"\n\nUPDATE \"public\".\"users\"\nSET \"email\" = 'new@example.com'\nWHERE \"id\" = '2';",
            sabiql::app::confirm_dialog_state::ConfirmIntent::ExecuteWrite {
                sql: "UPDATE \"public\".\"users\"\nSET \"email\" = 'new@example.com'\nWHERE \"id\" = '2';".to_string(),
                blocked: false,
            },
        );

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn confirm_dialog_update_preview_rich() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);

        let sql =
            "UPDATE \"public\".\"users\"\nSET \"email\" = 'new@example.com'\nWHERE \"id\" = '2';"
                .to_string();
        state.result_interaction.set_write_preview(WritePreview {
            operation: WriteOperation::Update,
            sql: sql.clone(),
            target_summary: TargetSummary {
                schema: "public".to_string(),
                table: "users".to_string(),
                key_values: vec![("id".to_string(), "2".to_string())],
            },
            diff: vec![ColumnDiff {
                column: "email".to_string(),
                before: "bob@example.com".to_string(),
                after: "new@example.com".to_string(),
            }],
            guardrail: GuardrailDecision {
                risk_level: RiskLevel::Low,
                blocked: false,
                reason: None,
                target_summary: None,
            },
        });
        state.modal.set_mode(InputMode::ConfirmDialog);
        state.confirm_dialog.open(
            "Confirm UPDATE: users",
            "email: \"bob@example.com\" -> \"new@example.com\"",
            sabiql::app::confirm_dialog_state::ConfirmIntent::ExecuteWrite {
                sql,
                blocked: false,
            },
        );

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn confirm_dialog_delete_preview_low_risk() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);

        let sql = "DELETE FROM \"public\".\"users\"\nWHERE \"id\" = '3';".to_string();
        state.result_interaction.set_write_preview(WritePreview {
            operation: WriteOperation::Delete,
            sql: sql.clone(),
            target_summary: TargetSummary {
                schema: "public".to_string(),
                table: "users".to_string(),
                key_values: vec![("id".to_string(), "3".to_string())],
            },
            diff: vec![],
            guardrail: GuardrailDecision {
                risk_level: RiskLevel::Low,
                blocked: false,
                reason: None,
                target_summary: None,
            },
        });
        state.modal.set_mode(InputMode::ConfirmDialog);
        state.confirm_dialog.open(
            "Confirm DELETE: users",
            "",
            sabiql::app::confirm_dialog_state::ConfirmIntent::ExecuteWrite {
                sql,
                blocked: false,
            },
        );

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }
}

mod inspector {
    use super::*;

    #[test]
    fn inspector_indexes_tab_with_data() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state.ui.inspector_tab = sabiql::app::inspector_tab::InspectorTab::Indexes;
        state.ui.focused_pane = FocusedPane::Inspector;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn inspector_foreign_keys_tab_with_data() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state.ui.inspector_tab = sabiql::app::inspector_tab::InspectorTab::ForeignKeys;
        state.ui.focused_pane = FocusedPane::Inspector;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn inspector_triggers_tab_with_data() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state.ui.inspector_tab = sabiql::app::inspector_tab::InspectorTab::Triggers;
        state.ui.focused_pane = FocusedPane::Inspector;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn inspector_triggers_tab_empty() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));

        let mut table = fixtures::sample_table_detail();
        table.triggers = vec![];
        let _ = state.session.set_table_detail(table, 0);
        state.ui.inspector_tab = sabiql::app::inspector_tab::InspectorTab::Triggers;
        state.ui.focused_pane = FocusedPane::Inspector;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn inspector_info_tab_shows_owner_and_comment() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state.ui.inspector_tab = sabiql::app::inspector_tab::InspectorTab::Info;
        state.ui.focused_pane = FocusedPane::Inspector;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn inspector_info_tab_with_no_metadata() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));

        let mut table = fixtures::sample_table_detail();
        table.owner = None;
        table.comment = None;
        table.row_count_estimate = None;
        let _ = state.session.set_table_detail(table, 0);
        state.ui.inspector_tab = sabiql::app::inspector_tab::InspectorTab::Info;
        state.ui.focused_pane = FocusedPane::Inspector;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }
}

mod result_pane {
    use super::*;

    #[test]
    fn result_pane_row_active_mode() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.enter_row(0);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn result_pane_cell_active_mode() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.enter_row(1);
        state.result_interaction.enter_cell(2);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn result_pane_cell_edit_mode() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.enter_row(1);
        state.result_interaction.enter_cell(2);
        state.modal.set_mode(InputMode::CellEdit);
        state
            .result_interaction
            .begin_cell_edit(1, 2, "bob@example.com".to_string());
        state
            .result_interaction
            .cell_edit_input_mut()
            .set_content("new@example.com".to_string());

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn result_pane_cell_edit_cursor_at_head() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.enter_row(1);
        state.result_interaction.enter_cell(2);
        state.modal.set_mode(InputMode::CellEdit);
        state
            .result_interaction
            .begin_cell_edit(1, 2, "bob@example.com".to_string());
        state.result_interaction.cell_edit_input_mut().set_cursor(0);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn result_pane_cell_edit_cursor_at_middle() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.enter_row(1);
        state.result_interaction.enter_cell(2);
        state.modal.set_mode(InputMode::CellEdit);
        state
            .result_interaction
            .begin_cell_edit(1, 2, "bob@example.com".to_string());
        state.result_interaction.cell_edit_input_mut().set_cursor(7);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn result_pane_cell_active_pending_draft() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.enter_row(1);
        state.result_interaction.enter_cell(2);
        state.modal.set_mode(InputMode::Normal);
        state
            .result_interaction
            .begin_cell_edit(1, 2, "bob@example.com".to_string());
        state
            .result_interaction
            .cell_edit_input_mut()
            .set_content("new@example.com".to_string());

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn result_pane_staged_delete_row() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.enter_row(0);
        state.result_interaction.stage_row(1);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }
}

mod result_history {
    use super::*;
    use sabiql::domain::QuerySource;

    fn adhoc_result(now: std::time::Instant, query: &str) -> sabiql::domain::QueryResult {
        sabiql::domain::QueryResult {
            query: query.to_string(),
            columns: vec!["count".to_string()],
            rows: vec![vec!["42".to_string()]],
            row_count: 1,
            execution_time_ms: 5,
            executed_at: now,
            source: QuerySource::Adhoc,
            error: None,
            command_tag: None,
        }
    }

    #[test]
    fn preview_with_history_hint() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        // Current result is Preview, but history has adhoc entries
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state
            .query
            .result_history
            .push(Arc::new(adhoc_result(now, "SELECT 1")));
        state.ui.focused_pane = FocusedPane::Result;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn result_pane_history_mode() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));

        // Push 3 adhoc results
        for i in 1..=3 {
            state
                .query
                .result_history
                .push(Arc::new(adhoc_result(now, &format!("SELECT {}", i))));
        }
        state
            .query
            .set_current_result(Arc::new(adhoc_result(now, "SELECT 3")));
        state.query.enter_history(1); // viewing 2/3
        state.ui.focused_pane = FocusedPane::Result;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    fn wide_adhoc_result(now: std::time::Instant, query: &str) -> sabiql::domain::QueryResult {
        sabiql::domain::QueryResult {
            query: query.to_string(),
            columns: (1..=10).map(|i| format!("column_{}", i)).collect(),
            rows: vec![(1..=10).map(|i| format!("value_{}", i)).collect()],
            row_count: 1,
            execution_time_ms: 12,
            executed_at: now,
            source: QuerySource::Adhoc,
            error: None,
            command_tag: None,
        }
    }

    #[test]
    fn history_mode_with_horizontal_scroll() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));

        let long_query = "SELECT column_1, column_2, column_3, column_4, column_5 FROM very_long_table_name WHERE id > 100";
        for i in 1..=3 {
            state
                .query
                .result_history
                .push(Arc::new(wide_adhoc_result(now, &format!("SELECT {}", i))));
        }
        state
            .query
            .set_current_result(Arc::new(wide_adhoc_result(now, long_query)));
        state.query.enter_history(2); // viewing 3/3
        state.ui.focus_mode = true;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn result_query_with_history_hint() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));

        // Push history but do NOT enter history mode (history_index = None)
        for i in 1..=2 {
            state
                .query
                .result_history
                .push(Arc::new(adhoc_result(now, &format!("SELECT {}", i))));
        }
        state
            .query
            .set_current_result(Arc::new(adhoc_result(now, "SELECT 2")));
        state.ui.focused_pane = FocusedPane::Result;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn focus_mode_history_mode() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.ui.set_explorer_selection(Some(0));

        for i in 1..=3 {
            state
                .query
                .result_history
                .push(Arc::new(adhoc_result(now, &format!("SELECT {}", i))));
        }
        state
            .query
            .set_current_result(Arc::new(adhoc_result(now, "SELECT 3")));
        state.query.enter_history(0); // viewing 1/3
        state.ui.focus_mode = true;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }
}

mod style_assertions {

    use super::*;
    use harness::{TEST_HEIGHT, TEST_WIDTH};
    use ratatui::style::{Color, Modifier};
    use sabiql::app::input_mode::InputMode;

    /// Help modal uses Percentage(70) x Percentage(80), centered in TEST_WIDTH x TEST_HEIGHT.
    fn help_modal_origin() -> (u16, u16) {
        let modal_w = TEST_WIDTH * 70 / 100;
        let modal_h = TEST_HEIGHT * 80 / 100;
        let x = (TEST_WIDTH - modal_w) / 2;
        let y = (TEST_HEIGHT - modal_h) / 2;
        (x, y)
    }

    #[test]
    fn pending_draft_cell_uses_orange_fg() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.enter_row(1);
        state.result_interaction.enter_cell(2);
        state.modal.set_mode(InputMode::Normal);
        state
            .result_interaction
            .begin_cell_edit(1, 2, "bob@example.com".to_string());
        state
            .result_interaction
            .cell_edit_input_mut()
            .set_content("new@example.com".to_string());

        let buffer = render_and_get_buffer(&mut terminal, &mut state);

        let orange = Color::Rgb(0xff, 0x99, 0x00);
        let draft_cell = (0..TEST_HEIGHT)
            .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
            .find(|&(x, y)| buffer.cell((x, y)).is_some_and(|c| c.fg == orange));
        assert!(
            draft_cell.is_some(),
            "Expected at least one cell with CELL_DRAFT_PENDING_FG (orange) in the buffer"
        );
    }

    #[test]
    fn active_cell_edit_uses_yellow_fg() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.enter_row(1);
        state.result_interaction.enter_cell(2);
        state.modal.set_mode(InputMode::CellEdit);
        state
            .result_interaction
            .begin_cell_edit(1, 2, "bob@example.com".to_string());
        state
            .result_interaction
            .cell_edit_input_mut()
            .set_content("new@example.com".to_string());

        let buffer = render_and_get_buffer(&mut terminal, &mut state);

        let yellow = Color::Yellow;
        let edit_cell = (0..TEST_HEIGHT)
            .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
            .find(|&(x, y)| buffer.cell((x, y)).is_some_and(|c| c.fg == yellow));
        assert!(
            edit_cell.is_some(),
            "Expected at least one cell with CELL_EDIT_FG (yellow) in the buffer"
        );
    }

    #[test]
    fn staged_delete_row_uses_dark_red_bg() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        let _ = state
            .session
            .set_table_detail(fixtures::sample_table_detail(), 0);
        state
            .query
            .set_current_result(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.enter_row(0);
        state.result_interaction.stage_row(1);

        let buffer = render_and_get_buffer(&mut terminal, &mut state);

        let dark_red = Color::Rgb(0x3d, 0x22, 0x22);
        let staged_cell = (0..TEST_HEIGHT)
            .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
            .find(|&(x, y)| buffer.cell((x, y)).is_some_and(|c| c.bg == dark_red));
        assert!(
            staged_cell.is_some(),
            "Expected at least one cell with STAGED_DELETE_BG (dark red) in the buffer"
        );
    }

    #[test]
    fn scrim_applies_dim_modifier() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.modal.set_mode(InputMode::Help);

        let buffer = render_and_get_buffer(&mut terminal, &mut state);

        let cell = buffer.cell((0, 0)).unwrap();
        assert!(
            cell.modifier.contains(Modifier::DIM),
            "Expected DIM modifier on scrim cell (0,0), got {:?}",
            cell.modifier
        );
    }

    #[test]
    fn modal_border_uses_ansi_darkgray() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state
            .session
            .mark_connected(Arc::new(fixtures::sample_metadata(now)));
        state.modal.set_mode(InputMode::Help);

        let buffer = render_and_get_buffer(&mut terminal, &mut state);

        let (mx, my) = help_modal_origin();
        let cell = buffer.cell((mx, my)).unwrap();
        assert_eq!(
            cell.symbol(),
            "╭",
            "Expected '╭' at modal origin ({}, {}), got '{}'",
            mx,
            my,
            cell.symbol()
        );
        assert_eq!(
            cell.fg,
            Color::DarkGray,
            "Expected DarkGray fg on modal border at ({}, {}), got {:?}",
            mx,
            my,
            cell.fg
        );
    }
}
