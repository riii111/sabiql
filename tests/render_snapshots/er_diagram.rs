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
    state.ui.er_picker.filter_input = "user".to_string();

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
