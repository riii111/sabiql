use super::*;
use harness::{table_detail_loaded_state, with_current_result};
use sabiql_app::model::app_state::AppState;
use sabiql_app::services::AppServices;
use sabiql_app::update::action::{Action, CursorMove, InputTarget, ModalKind};
use sabiql_app::update::browse::result::dispatch_result;
use sabiql_domain::{Column, ColumnAttributes, QueryResult};

fn jsonb_detail_state() -> (AppState, std::time::Instant) {
    let now = test_instant();
    let mut state = create_test_state();
    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata()));
    let mut table = fixtures::sample_table_detail();
    table.columns.push(Column {
        name: "settings".to_string(),
        data_type: "jsonb".to_string(),
        attributes: ColumnAttributes::NULLABLE,
        default: None,
        comment: None,
        ordinal_position: 4,
    });
    let _ = state.session.set_table_detail(table, 0);
    state
        .query
        .set_current_result(Arc::new(QueryResult::success(
            "SELECT id, name, email, settings FROM users LIMIT 100".to_string(),
            vec![
                "id".to_string(),
                "name".to_string(),
                "email".to_string(),
                "settings".to_string(),
            ],
            vec![vec![
                "1".to_string(),
                "Alice".to_string(),
                "alice@example.com".to_string(),
                r#"{"theme":"dark","count":5,"nested":{"enabled":true,"roles":["admin","writer"]}}"#
                    .to_string(),
            ]],
            1,
            QuerySource::Preview,
        )));
    state.query.pagination.reset_for_table("public", "users");
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(0, 3);
    (state, now)
}

fn cell_detail_state() -> (AppState, std::time::Instant) {
    let now = test_instant();
    let mut state = table_detail_loaded_state();
    state
        .query
        .set_current_result(Arc::new(QueryResult::success(
            "SELECT id, body FROM notes".to_string(),
            vec!["id".to_string(), "body".to_string()],
            vec![vec![
                "1".to_string(),
                "Prompt:\nSummarize the incident timeline and include the operator notes.\n\nMemory:\n- User prefers concise status updates\n- Keep markdown bullets intact".to_string(),
            ]],
            1,
            QuerySource::Preview,
        )));
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(0, 1);
    (state, now)
}

fn row_detail_state() -> (AppState, std::time::Instant) {
    let now = test_instant();
    let mut state = create_test_state();
    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata()));
    state
        .query
        .set_current_result(Arc::new(QueryResult::success(
            "SELECT id, name, email, active FROM users LIMIT 100".to_string(),
            vec![
                "id".to_string(),
                "name".to_string(),
                "email".to_string(),
                "active".to_string(),
            ],
            vec![vec![
                "1".to_string(),
                "Alice".to_string(),
                "alice@example.com".to_string(),
                "true".to_string(),
            ]],
            1,
            QuerySource::Preview,
        )));
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(0, 0);
    (state, now)
}

#[test]
fn result_pane_scrolled_past_wide_column_fills_width() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    // payload's ideal width nearly fills the pane; scrolled past it, the
    // remaining narrow columns must fill the viewport instead of leaving
    // most of the pane blank
    state
        .query
        .set_current_result(Arc::new(QueryResult::success(
            "SELECT * FROM events".to_string(),
            ["id", "payload", "status", "kind", "actor", "note"]
                .iter()
                .map(ToString::to_string)
                .collect(),
            vec![
                vec![
                    "1".to_string(),
                    "x".repeat(100),
                    "active_pending_validation".to_string(),
                    "user_account_registration".to_string(),
                    "alice.anderson@example.com".to_string(),
                    "created via admin console".to_string(),
                ],
                vec![
                    "2".to_string(),
                    "y".repeat(100),
                    "suspended_awaiting_review".to_string(),
                    "service_account_creation".to_string(),
                    "bob.brown@example.com".to_string(),
                    "imported from legacy system".to_string(),
                ],
            ],
            3,
            QuerySource::Preview,
        )));
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.set_horizontal_offset(2);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_right_edge_peeks_truncated_previous_column() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    // At the right edge the trailing columns leave leftover width; the
    // hidden wide description column shows up truncated instead of blank
    state
        .query
        .set_current_result(Arc::new(QueryResult::success(
            "SELECT * FROM events".to_string(),
            ["id", "description", "status", "kind", "actor", "note"]
                .iter()
                .map(ToString::to_string)
                .collect(),
            vec![
                vec![
                    "1".to_string(),
                    "x".repeat(100),
                    "active_validation".to_string(),
                    "create_operation".to_string(),
                    "alice.anderson".to_string(),
                    "first_revision".to_string(),
                ],
                vec![
                    "2".to_string(),
                    "y".repeat(100),
                    "suspended_review".to_string(),
                    "update_operation".to_string(),
                    "bob.brownfield".to_string(),
                    "second_revision".to_string(),
                ],
            ],
            3,
            QuerySource::Preview,
        )));
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.set_horizontal_offset(2);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_narrow_pane_keeps_horizontal_scroll() {
    let mut state = table_detail_loaded_state();
    // Split-pane terminal: the two payload columns exceed the pane width even
    // after capping, which must not disable the scrollbar
    let mut terminal = create_test_terminal_sized(110, 40);

    state
        .query
        .set_current_result(Arc::new(QueryResult::success(
            "SELECT * FROM events".to_string(),
            ["id", "payload", "details", "status"]
                .iter()
                .map(ToString::to_string)
                .collect(),
            vec![
                vec![
                    "1".to_string(),
                    "x".repeat(100),
                    "z".repeat(100),
                    "active".to_string(),
                ],
                vec![
                    "2".to_string(),
                    "y".repeat(100),
                    "w".repeat(100),
                    "done".to_string(),
                ],
            ],
            3,
            QuerySource::Preview,
        )));
    state.ui.set_focused_pane(FocusedPane::Result);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_first_cell_active_mode() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(0, 0);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_active_mode() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(1, 2);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_edit_mode() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(1, 2);
    state.modal.set_mode(InputMode::CellEdit);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    state
        .result_interaction
        .replace_cell_edit_draft("new@example.com".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_edit_cursor_at_head() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(1, 2);
    state.modal.set_mode(InputMode::CellEdit);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    state.result_interaction.cell_edit_set_cursor(0);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_edit_cursor_at_middle() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(1, 2);
    state.modal.set_mode(InputMode::CellEdit);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    state.result_interaction.cell_edit_set_cursor(7);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_active_pending_draft() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(1, 2);
    state.modal.set_mode(InputMode::Normal);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    state
        .result_interaction
        .replace_cell_edit_draft("new@example.com".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_edit_cursor_at_tail() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(1, 2);
    state.modal.set_mode(InputMode::CellEdit);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    let len = state.result_interaction.cell_edit().input().content().len();
    state.result_interaction.cell_edit_set_cursor(len);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_staged_delete_row() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(0, 0);
    state.result_interaction.stage_row(1);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_jsonb_detail_mode() {
    let (mut state, now) = jsonb_detail_state();
    let mut terminal = create_test_terminal();

    dispatch_result(
        &mut state,
        &Action::OpenModal(ModalKind::JsonbDetail),
        &AppServices::stub(),
        now,
    );
    assert_eq!(state.input_mode(), InputMode::JsonbDetail);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_detail_mode() {
    let (mut state, now) = cell_detail_state();
    let mut terminal = create_test_terminal();

    dispatch_result(
        &mut state,
        &Action::ResultOpenCellDetail,
        &AppServices::stub(),
        now,
    );
    assert_eq!(state.input_mode(), InputMode::CellDetail);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_jsonb_detail_shows_vertical_scrollbar() {
    let (mut state, now) = jsonb_detail_state();
    let mut terminal = create_test_terminal_sized(100, 25);
    let long_json = format!(
        "{{{}}}",
        (0..40)
            .map(|i| format!(r#""key_{i}":"value_{i}""#))
            .collect::<Vec<_>>()
            .join(",")
    );
    state
        .query
        .set_current_result(Arc::new(QueryResult::success(
            "SELECT id, name, email, settings FROM users LIMIT 1".to_string(),
            vec![
                "id".to_string(),
                "name".to_string(),
                "email".to_string(),
                "settings".to_string(),
            ],
            vec![vec![
                "1".to_string(),
                "Alice".to_string(),
                "alice@example.com".to_string(),
                long_json,
            ]],
            1,
            QuerySource::Preview,
        )));
    state.result_interaction.activate_cell(0, 3);

    dispatch_result(
        &mut state,
        &Action::OpenModal(ModalKind::JsonbDetail),
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

    dispatch_result(
        &mut state,
        &Action::OpenModal(ModalKind::JsonbDetail),
        &AppServices::stub(),
        now,
    );
    assert_eq!(state.input_mode(), InputMode::JsonbDetail);
    dispatch_result(
        &mut state,
        &Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Down,
        },
        &AppServices::stub(),
        now,
    );
    dispatch_result(
        &mut state,
        &Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Right,
        },
        &AppServices::stub(),
        now,
    );
    dispatch_result(
        &mut state,
        &Action::JsonbEnterEdit,
        &AppServices::stub(),
        now,
    );
    assert_eq!(state.input_mode(), InputMode::JsonbEdit);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_row_detail_shows_scrollbars() {
    let (mut state, now) = row_detail_state();
    let mut terminal = create_test_terminal_sized(100, 25);
    let body = (0..40)
        .map(|i| format!("line {i}: {}", "x".repeat(120)))
        .collect::<Vec<_>>()
        .join("\n");
    state
        .query
        .set_current_result(Arc::new(QueryResult::success(
            "SELECT body FROM logs LIMIT 1".to_string(),
            vec!["body".to_string()],
            vec![vec![body]],
            1,
            QuerySource::Preview,
        )));
    state.result_interaction.activate_cell(0, 0);

    dispatch_result(
        &mut state,
        &Action::OpenModal(ModalKind::RowDetail),
        &AppServices::stub(),
        now,
    );
    assert_eq!(state.input_mode(), InputMode::RowDetail);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_row_detail_mode() {
    let (mut state, now) = row_detail_state();
    let mut terminal = create_test_terminal();

    dispatch_result(
        &mut state,
        &Action::OpenModal(ModalKind::RowDetail),
        &AppServices::stub(),
        now,
    );
    assert_eq!(state.input_mode(), InputMode::RowDetail);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
