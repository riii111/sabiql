use super::*;
use app::model::shared::confirm_dialog::ConfirmIntent;
use domain::connection::ServiceEntry;
use domain::connection::{ConnectionId, ConnectionName, ConnectionProfile, SslMode};

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
    use domain::connection::ServiceEntry;

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
        ConfirmIntent::DeleteConnection(connection_id),
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
        ConfirmIntent::DeleteConnection(target_id),
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
