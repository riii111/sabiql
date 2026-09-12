use super::*;
use harness::explorer_selected_state;

fn picker_rows(output: &str) -> Vec<&str> {
    let lines = output.lines().collect::<Vec<_>>();
    let mode = lines
        .iter()
        .position(|line| line.contains("Mode:"))
        .expect("expected ER picker mode row");
    lines[mode..].iter().take(6).copied().collect()
}

#[test]
fn er_waiting_progress() {
    let mut state = explorer_selected_state();
    let mut terminal = create_test_terminal();

    let _ = state.er_preparation.start_waiting_run();
    state.er_preparation.begin_all_prefetch([
        "public.users".to_string(),
        "public.comments".to_string(),
        "public.posts".to_string(),
    ]);
    state
        .table_prefetch
        .queue_table_prefetch("public.comments".to_string());
    state
        .table_prefetch
        .start_table_prefetch("public.posts".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_modal() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_filtered() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);
    state.ui.er_picker_mut().insert_filter_str("user");

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_single_select() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);
    state
        .ui
        .replace_er_selected_tables(["public.users".to_string()]);

    let output = render_to_string(&mut terminal, &mut state);
    let rows = picker_rows(&output);
    assert!(rows[0].contains("Mode:    Partial ER"));
    assert!(rows[1].contains("Targets: public.users"));
    assert!(rows[2].contains("Output:  er_partial_public_users.dot"));
    assert!(rows[3].contains("✔ public.users"));
    assert!(!rows[4].contains("✔"));
    assert!(output.lines().any(|line| line.contains("1/3 selected")));
}

#[test]
fn er_table_picker_multi_select() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);
    state
        .ui
        .replace_er_selected_tables(["public.users".to_string(), "public.posts".to_string()]);

    let output = render_to_string(&mut terminal, &mut state);
    let rows = picker_rows(&output);
    assert!(rows[0].contains("Mode:    Partial ER"));
    assert!(rows[1].contains("Targets: 2 tables"));
    assert!(rows[2].contains("Output:  er_partial_multi_2_89466781.dot"));
    assert!(rows[3].contains("✔ public.users"));
    assert!(rows[4].contains("✔ public.posts"));
    assert!(!rows[5].contains("✔"));
    assert!(output.lines().any(|line| line.contains("2/3 selected")));
}

#[test]
fn er_table_picker_all_selected() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);
    state.ui.replace_er_selected_tables([
        "public.users".to_string(),
        "public.posts".to_string(),
        "public.comments".to_string(),
    ]);

    let output = render_to_string(&mut terminal, &mut state);
    let rows = picker_rows(&output);
    assert!(rows[0].contains("Mode:    Full ER"));
    assert!(rows[1].contains("Targets: all 3 tables"));
    assert!(rows[2].contains("Output:  er_full.dot"));
    assert!(rows[3].contains("✔ public.users"));
    assert!(rows[4].contains("✔ public.posts"));
    assert!(rows[5].contains("✔ public.comments"));
    assert!(output.lines().any(|line| line.contains("3/3 selected")));
}

#[test]
fn mysql_er_table_picker_shows_table_names_without_database() {
    let mut state = super::harness::create_test_state();
    state.session.activate_connection_with_target(
        &sabiql_domain::ConnectionId::from_string("mysql-test"),
        "mysql",
        sabiql_domain::DatabaseType::MySQL,
        "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
        Some("app"),
    );
    state.session.mark_connected(std::sync::Arc::new(
        super::harness::fixtures::sample_metadata(),
    ));
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn mysql_er_table_picker_single_select_shows_table_name_without_database() {
    let mut state = super::harness::create_test_state();
    state.session.activate_connection_with_target(
        &sabiql_domain::ConnectionId::from_string("mysql-test"),
        "mysql",
        sabiql_domain::DatabaseType::MySQL,
        "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
        Some("app"),
    );
    let mut metadata = super::harness::fixtures::sample_metadata();
    for table in &mut metadata.table_summaries {
        table.schema = "app".to_string();
    }
    state.session.mark_connected(std::sync::Arc::new(metadata));
    state.modal.set_mode(InputMode::ErTablePicker);
    state
        .ui
        .replace_er_selected_tables(["app.users".to_string()]);
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
