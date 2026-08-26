use super::*;
use sabiql_app::model::shared::confirm_dialog::ConfirmIntent;
use sabiql_domain::connection::ServiceEntry;
use sabiql_domain::connection::{ConnectionId, ConnectionProfile, SslMode};

fn three_connections() -> (ConnectionId, Vec<ConnectionProfile>) {
    let active_id = ConnectionId::new();
    let profiles = vec![
        ConnectionProfile::with_id_postgres(
            active_id.clone(),
            "Production",
            "prod.example.com",
            5432,
            "prod_db",
            "admin",
            "secret",
            SslMode::Require,
        )
        .unwrap(),
        ConnectionProfile::new_postgres(
            "Staging",
            "staging.example.com",
            5432,
            "staging_db",
            "user",
            "pass",
            SslMode::Prefer,
        )
        .unwrap(),
        ConnectionProfile::new_postgres(
            "Local Dev",
            "localhost",
            5432,
            "dev_db",
            "dev",
            "dev",
            SslMode::Disable,
        )
        .unwrap(),
    ];
    (active_id, profiles)
}

#[test]
fn connection_selector_with_multiple_connections() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    let (active_id, connections) = three_connections();
    state.set_connections(connections);
    state.session.set_active_connection_identity_for_test(
        &active_id,
        "localhost:5432/test",
        sabiql_domain::DatabaseType::PostgreSQL,
    );
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
            },
            ServiceEntry {
                service_name: "prod-replica".to_string(),
            },
        ],
    );
    state.session.set_active_connection_identity_for_test(
        &active_id,
        "localhost:5432/test",
        sabiql_domain::DatabaseType::PostgreSQL,
    );
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
        },
        ServiceEntry {
            service_name: "short".to_string(),
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
        },
        ServiceEntry {
            service_name: "prod-replica".to_string(),
        },
    ]);
    // Set active connection to the first service entry
    state.session.set_active_connection_identity_for_test(
        &ConnectionId::from_string("service:dev-local".to_string()),
        "localhost:5432/test",
        sabiql_domain::DatabaseType::PostgreSQL,
    );
    state.modal.set_mode(InputMode::ConnectionSelector);
    state.ui.set_connection_list_selection(Some(0));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_selector_with_multibyte_service_name() {
    use sabiql_domain::connection::ServiceEntry;

    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.set_service_entries(vec![ServiceEntry {
        service_name: "本番データベース接続".to_string(),
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
