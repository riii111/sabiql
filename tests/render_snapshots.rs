mod harness;

use harness::fixtures;
use harness::{create_test_state, create_test_terminal, render_to_string, test_instant};

use std::sync::Arc;
use std::time::Duration;

use sabiql::app::action::Action;
use sabiql::app::connection_error::{ConnectionErrorInfo, ConnectionErrorKind};
use sabiql::app::connection_setup_state::ConnectionField;
use sabiql::app::er_state::ErStatus;
use sabiql::app::focused_pane::FocusedPane;
use sabiql::app::input_mode::InputMode;
use sabiql::app::sql_modal_context::SqlModalStatus;
use sabiql::domain::MetadataState;

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
fn footer_shows_success_message() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.messages.last_success = Some("Reconnected!".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn explorer_connections_mode() {
    use sabiql::app::explorer_mode::ExplorerMode;
    use sabiql::domain::connection::{ConnectionId, ConnectionName, ConnectionProfile, SslMode};

    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    // Set up connections
    let active_id = ConnectionId::new();
    state.connections = vec![
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

    // Set active connection
    state.runtime.active_connection_id = Some(active_id);

    // Switch to Connections mode
    state.ui.explorer_mode = ExplorerMode::Connections;
    state.ui.focused_pane = FocusedPane::Explorer;
    state.ui.set_connection_list_selection(Some(0));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_selector_with_multiple_connections() {
    use sabiql::domain::connection::{ConnectionId, ConnectionName, ConnectionProfile, SslMode};

    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    let active_id = ConnectionId::new();
    state.connections = vec![
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

    state.runtime.active_connection_id = Some(active_id);
    state.ui.input_mode = InputMode::ConnectionSelector;
    state.ui.set_connection_list_selection(Some(0));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn confirm_dialog_delete_active_connection() {
    use sabiql::domain::connection::ConnectionId;

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
    use sabiql::domain::connection::ConnectionId;

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
    use sabiql::app::explorer_mode::ExplorerMode;

    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    // No connections configured
    state.connections = vec![];
    state.runtime.active_connection_id = None;

    // Switch to Connections mode
    state.ui.explorer_mode = ExplorerMode::Connections;
    state.ui.focused_pane = FocusedPane::Explorer;
    state.ui.set_connection_list_selection(None);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
