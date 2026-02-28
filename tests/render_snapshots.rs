mod harness;

use harness::fixtures;
use harness::{
    create_test_state, create_test_terminal, render_and_get_buffer, render_to_string, test_instant,
};

use std::sync::Arc;
use std::time::Duration;

use sabiql::app::action::Action;
use sabiql::app::connection_error::{ConnectionErrorInfo, ConnectionErrorKind};
use sabiql::app::connection_setup_state::ConnectionField;
use sabiql::app::er_state::ErStatus;
use sabiql::app::focused_pane::FocusedPane;
use sabiql::app::input_mode::InputMode;
use sabiql::app::sql_modal_context::SqlModalStatus;
use sabiql::app::write_guardrails::{
    ColumnDiff, GuardrailDecision, RiskLevel, TargetSummary, WriteOperation, WritePreview,
};
use sabiql::domain::MetadataState;

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
        state.runtime.active_connection_name = None;
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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.cache.table_detail = Some(fixtures::sample_table_detail());
        state.query.current_result = Some(Arc::new(fixtures::sample_query_result(now)));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn focus_on_result_pane() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.query.current_result = Some(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn focus_mode_fullscreen_result() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.query.current_result = Some(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focus_mode = true;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn error_message_in_footer() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.cache.table_detail = Some(fixtures::sample_table_detail());
        state.query.current_result = Some(Arc::new(fixtures::empty_query_result(now)));

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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.input_mode = InputMode::SqlModal;
        state.sql_modal.content = "SELECT * FROM us".to_string();
        state.sql_modal.cursor = 16;
        state.sql_modal.status = SqlModalStatus::Editing;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn help_overlay() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.input_mode = InputMode::Help;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn command_palette_overlay() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.input_mode = InputMode::CommandPalette;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn table_picker_overlay() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.input_mode = InputMode::TablePicker;
        state.ui.filter_input = "user".to_string();

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn command_line_input() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.input_mode = InputMode::CommandLine;
        state.command_line_input = "sql".to_string();

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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.input_mode = InputMode::ErTablePicker;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn er_table_picker_filtered() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.input_mode = InputMode::ErTablePicker;
        state.ui.er_filter_input = "user".to_string();

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn er_table_picker_single_select() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.input_mode = InputMode::ErTablePicker;
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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.input_mode = InputMode::ErTablePicker;
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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.input_mode = InputMode::ErTablePicker;
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

        state.ui.input_mode = InputMode::ConnectionSetup;
        state.connection_setup.database = "mydb".to_string();
        state.connection_setup.user = "postgres".to_string();

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_setup_with_validation_errors() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.ui.input_mode = InputMode::ConnectionSetup;
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

        state.ui.input_mode = InputMode::ConnectionError;
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

        state.ui.input_mode = InputMode::ConnectionError;
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

        state.ui.input_mode = InputMode::ConnectionError;
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
    use sabiql::app::explorer_mode::ExplorerMode;
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
    fn explorer_connections_mode() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        let (active_id, connections) = three_connections();
        let count = connections.len();
        state.connections = connections;
        state.connection_list_items = sabiql::app::connection_list::build_connection_list(count, 0);
        state.runtime.active_connection_id = Some(active_id);
        state.ui.explorer_mode = ExplorerMode::Connections;
        state.ui.focused_pane = FocusedPane::Explorer;
        state.ui.set_connection_list_selection(Some(0));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_selector_with_multiple_connections() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        let (active_id, connections) = three_connections();
        let count = connections.len();
        state.connections = connections;
        state.connection_list_items = sabiql::app::connection_list::build_connection_list(count, 0);
        state.runtime.active_connection_id = Some(active_id);
        state.ui.input_mode = InputMode::ConnectionSelector;
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
        let profile_count = connections.len();
        state.connections = connections;
        state.service_entries = vec![
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
        ];
        let service_count = state.service_entries.len();
        state.connection_list_items =
            sabiql::app::connection_list::build_connection_list(profile_count, service_count);
        state.runtime.active_connection_id = Some(active_id);
        state.ui.input_mode = InputMode::ConnectionSelector;
        state.ui.set_connection_list_selection(Some(0));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn connection_selector_with_long_service_name() {
        use sabiql::domain::connection::ServiceEntry;

        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.service_entries = vec![
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
        ];
        let service_count = state.service_entries.len();
        state.connection_list_items =
            sabiql::app::connection_list::build_connection_list(0, service_count);
        state.ui.input_mode = InputMode::ConnectionSelector;
        state.ui.set_connection_list_selection(Some(0));

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn confirm_dialog_delete_active_connection() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        // ID is for Action payload only; test verifies warning message UI
        let connection_id = ConnectionId::new();
        state.ui.input_mode = InputMode::ConfirmDialog;
        state.confirm_dialog.title = "Delete Connection".to_string();
        state.confirm_dialog.message =
            "Delete \"Production\"?\n\n\u{26A0} This is the active connection.\nYou will be disconnected.\n\nThis action cannot be undone.".to_string();
        state.confirm_dialog.on_confirm = Action::DeleteConnection(connection_id);
        state.confirm_dialog.on_cancel = Action::None;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn confirm_dialog_delete_inactive_connection() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        let target_id = ConnectionId::new();
        state.ui.input_mode = InputMode::ConfirmDialog;
        state.confirm_dialog.title = "Delete Connection".to_string();
        state.confirm_dialog.message =
            "Delete \"Staging\"?\n\nThis action cannot be undone.".to_string();
        state.confirm_dialog.on_confirm = Action::DeleteConnection(target_id);
        state.confirm_dialog.on_cancel = Action::None;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn explorer_connections_mode_empty() {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.connections = vec![];
        state.runtime.active_connection_id = None;
        state.ui.explorer_mode = ExplorerMode::Connections;
        state.ui.focused_pane = FocusedPane::Explorer;
        state.ui.set_connection_list_selection(None);

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

        state.ui.input_mode = InputMode::ConfirmDialog;
        state.confirm_dialog.title = "Confirm".to_string();
        state.confirm_dialog.message =
            "No connection configured.\nAre you sure you want to quit?".to_string();
        state.confirm_dialog.on_confirm = Action::Quit;
        state.confirm_dialog.on_cancel = Action::OpenConnectionSetup;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn confirm_dialog_update_preview() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.cache.table_detail = Some(fixtures::sample_table_detail());
        state.ui.input_mode = InputMode::ConfirmDialog;
        state.confirm_dialog.title = "Confirm UPDATE: users".to_string();
        state.confirm_dialog.message =
            "email: \"bob@example.com\" -> \"new@example.com\"\n\nUPDATE \"public\".\"users\"\nSET \"email\" = 'new@example.com'\nWHERE \"id\" = '2';".to_string();
        state.confirm_dialog.on_confirm = Action::ExecuteWrite(
            "UPDATE \"public\".\"users\"\nSET \"email\" = 'new@example.com'\nWHERE \"id\" = '2';"
                .to_string(),
        );
        state.confirm_dialog.on_cancel = Action::None;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn confirm_dialog_update_preview_rich() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.cache.table_detail = Some(fixtures::sample_table_detail());

        let sql =
            "UPDATE \"public\".\"users\"\nSET \"email\" = 'new@example.com'\nWHERE \"id\" = '2';"
                .to_string();
        state.pending_write_preview = Some(WritePreview {
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
        state.ui.input_mode = InputMode::ConfirmDialog;
        state.confirm_dialog.title = "Confirm UPDATE: users".to_string();
        state.confirm_dialog.message =
            "email: \"bob@example.com\" -> \"new@example.com\"".to_string();
        state.confirm_dialog.on_confirm = Action::ExecuteWrite(sql);
        state.confirm_dialog.on_cancel = Action::None;

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn confirm_dialog_delete_preview_low_risk() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.cache.table_detail = Some(fixtures::sample_table_detail());

        let sql = "DELETE FROM \"public\".\"users\"\nWHERE \"id\" = '3';".to_string();
        state.pending_write_preview = Some(WritePreview {
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
        state.ui.input_mode = InputMode::ConfirmDialog;
        state.confirm_dialog.title = "Confirm DELETE: users".to_string();
        state.confirm_dialog.message = String::new();
        state.confirm_dialog.on_confirm = Action::ExecuteWrite(sql);
        state.confirm_dialog.on_cancel = Action::None;

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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.cache.table_detail = Some(fixtures::sample_table_detail());
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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.cache.table_detail = Some(fixtures::sample_table_detail());
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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.cache.table_detail = Some(fixtures::sample_table_detail());
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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));

        let mut table = fixtures::sample_table_detail();
        table.triggers = vec![];
        state.cache.table_detail = Some(table);
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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.cache.table_detail = Some(fixtures::sample_table_detail());
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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));

        let mut table = fixtures::sample_table_detail();
        table.owner = None;
        table.comment = None;
        table.row_count_estimate = None;
        state.cache.table_detail = Some(table);
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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.cache.table_detail = Some(fixtures::sample_table_detail());
        state.query.current_result = Some(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.ui.result_selection.enter_row(0);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn result_pane_cell_active_mode() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.cache.table_detail = Some(fixtures::sample_table_detail());
        state.query.current_result = Some(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.ui.result_selection.enter_row(1);
        state.ui.result_selection.enter_cell(2);

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn result_pane_cell_edit_mode() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.cache.table_detail = Some(fixtures::sample_table_detail());
        state.query.current_result = Some(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.ui.result_selection.enter_row(1);
        state.ui.result_selection.enter_cell(2);
        state.ui.input_mode = InputMode::CellEdit;
        state.cell_edit.begin(1, 2, "bob@example.com".to_string());
        state.cell_edit.draft_value = "new@example.com".to_string();

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn result_pane_cell_active_pending_draft() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.cache.table_detail = Some(fixtures::sample_table_detail());
        state.query.current_result = Some(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.ui.result_selection.enter_row(1);
        state.ui.result_selection.enter_cell(2);
        state.ui.input_mode = InputMode::Normal;
        state.cell_edit.begin(1, 2, "bob@example.com".to_string());
        state.cell_edit.draft_value = "new@example.com".to_string();

        let output = render_to_string(&mut terminal, &mut state);

        insta::assert_snapshot!(output);
    }

    #[test]
    fn result_pane_staged_delete_row() {
        let now = test_instant();
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = sabiql::domain::MetadataState::Loaded;
        state.ui.set_explorer_selection(Some(0));
        state.cache.table_detail = Some(fixtures::sample_table_detail());
        state.query.current_result = Some(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.ui.result_selection.enter_row(0);
        state.ui.staged_delete_rows.insert(1);

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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = sabiql::domain::MetadataState::Loaded;
        state.cache.table_detail = Some(fixtures::sample_table_detail());
        state.query.current_result = Some(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.ui.result_selection.enter_row(1);
        state.ui.result_selection.enter_cell(2);
        state.ui.input_mode = InputMode::Normal;
        state.cell_edit.begin(1, 2, "bob@example.com".to_string());
        state.cell_edit.draft_value = "new@example.com".to_string();

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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = sabiql::domain::MetadataState::Loaded;
        state.cache.table_detail = Some(fixtures::sample_table_detail());
        state.query.current_result = Some(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.ui.result_selection.enter_row(1);
        state.ui.result_selection.enter_cell(2);
        state.ui.input_mode = InputMode::CellEdit;
        state.cell_edit.begin(1, 2, "bob@example.com".to_string());
        state.cell_edit.draft_value = "new@example.com".to_string();

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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = sabiql::domain::MetadataState::Loaded;
        state.cache.table_detail = Some(fixtures::sample_table_detail());
        state.query.current_result = Some(Arc::new(fixtures::sample_query_result(now)));
        state.ui.focused_pane = FocusedPane::Result;
        state.ui.result_selection.enter_row(0);
        state.ui.staged_delete_rows.insert(1);

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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = sabiql::domain::MetadataState::Loaded;
        state.ui.input_mode = InputMode::Help;

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

        state.cache.metadata = Some(fixtures::sample_metadata(now));
        state.cache.state = sabiql::domain::MetadataState::Loaded;
        state.ui.input_mode = InputMode::Help;

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
