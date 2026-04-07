use super::*;
use harness::{table_detail_loaded_state, with_current_result};
use sabiql::app::services::AppServices;
use sabiql::app::update::action::Action;
use sabiql::app::update::browse::result::reduce_result;
use sabiql::domain::{Column, QueryResult};

fn jsonb_detail_state() -> (sabiql::app::model::app_state::AppState, std::time::Instant) {
    let now = test_instant();
    let mut state = create_test_state();
    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata(now)));
    let mut table = fixtures::sample_table_detail();
    table.columns.push(Column {
        name: "settings".to_string(),
        data_type: "jsonb".to_string(),
        nullable: true,
        is_primary_key: false,
        is_unique: false,
        default: None,
        comment: None,
        ordinal_position: 4,
    });
    let _ = state.session.set_table_detail(table, 0);
    state.query.set_current_result(Arc::new(QueryResult {
        query: "SELECT id, name, email, settings FROM users LIMIT 100".to_string(),
        columns: vec![
            "id".to_string(),
            "name".to_string(),
            "email".to_string(),
            "settings".to_string(),
        ],
        rows: vec![vec![
            "1".to_string(),
            "Alice".to_string(),
            "alice@example.com".to_string(),
            r#"{"theme":"dark","count":5,"nested":{"enabled":true,"roles":["admin","writer"]}}"#
                .to_string(),
        ]],
        row_count: 1,
        execution_time_ms: 1,
        executed_at: now,
        source: QuerySource::Preview,
        error: None,
        command_tag: None,
    }));
    state.query.pagination.schema = "public".to_string();
    state.query.pagination.table = "users".to_string();
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.activate_cell(0, 3);
    (state, now)
}

#[test]
fn result_pane_first_cell_active_mode() {
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.activate_cell(0, 0);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_active_mode() {
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.activate_cell(1, 2);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_edit_mode() {
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.activate_cell(1, 2);
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
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.activate_cell(1, 2);
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
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.activate_cell(1, 2);
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
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.activate_cell(1, 2);
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
fn result_pane_cell_edit_cursor_at_tail() {
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.activate_cell(1, 2);
    state.modal.set_mode(InputMode::CellEdit);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    let len = state.result_interaction.cell_edit().input.content().len();
    state
        .result_interaction
        .cell_edit_input_mut()
        .set_cursor(len);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_staged_delete_row() {
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.activate_cell(0, 0);
    state.result_interaction.stage_row(1);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_jsonb_detail_mode() {
    let (mut state, now) = jsonb_detail_state();
    let mut terminal = create_test_terminal();

    reduce_result(
        &mut state,
        &Action::OpenJsonbDetail,
        &AppServices::stub(),
        now,
    );
    assert_eq!(state.input_mode(), InputMode::JsonbDetail);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_jsonb_edit_mode() {
    let (mut state, now) = jsonb_detail_state();
    let mut terminal = create_test_terminal();

    reduce_result(
        &mut state,
        &Action::OpenJsonbDetail,
        &AppServices::stub(),
        now,
    );
    assert_eq!(state.input_mode(), InputMode::JsonbDetail);
    reduce_result(
        &mut state,
        &Action::TextMoveCursor {
            target: sabiql::app::update::action::InputTarget::JsonbEdit,
            direction: sabiql::app::update::action::CursorMove::Down,
        },
        &AppServices::stub(),
        now,
    );
    reduce_result(
        &mut state,
        &Action::TextMoveCursor {
            target: sabiql::app::update::action::InputTarget::JsonbEdit,
            direction: sabiql::app::update::action::CursorMove::Right,
        },
        &AppServices::stub(),
        now,
    );
    reduce_result(
        &mut state,
        &Action::JsonbEnterEdit,
        &AppServices::stub(),
        now,
    );
    assert_eq!(state.input_mode(), InputMode::JsonbEdit);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
