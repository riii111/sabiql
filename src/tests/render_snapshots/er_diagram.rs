use super::*;
use harness::{connected_state, explorer_selected_state};

#[test]
fn er_waiting_progress() {
    let (mut state, _now) = explorer_selected_state();
    let mut terminal = create_test_terminal();

    state.er_preparation.start_waiting_run();
    state.er_preparation.begin_full_prefetch(3);
    state
        .er_preparation
        .insert_pending_table("public.comments".to_string());
    state
        .er_preparation
        .insert_fetching_table("public.posts".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_modal() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_filtered() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);
    state.ui.er_picker_mut().insert_filter_str("user");

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_single_select() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);
    state
        .ui
        .replace_er_selected_tables(["public.users".to_string()]);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_multi_select() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);
    state
        .ui
        .replace_er_selected_tables(["public.users".to_string(), "public.posts".to_string()]);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_all_selected() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);
    state.ui.replace_er_selected_tables([
        "public.users".to_string(),
        "public.posts".to_string(),
        "public.comments".to_string(),
    ]);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
