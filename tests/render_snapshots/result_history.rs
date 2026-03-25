use super::*;
use harness::{explorer_selected_state, table_detail_loaded_state, with_current_result};
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
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    // Current result is Preview, but history has adhoc entries
    with_current_result(&mut state, now);
    state
        .query
        .push_history(Arc::new(adhoc_result(now, "SELECT 1")));
    state.ui.focused_pane = FocusedPane::Result;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_history_mode() {
    let (mut state, now) = explorer_selected_state();
    let mut terminal = create_test_terminal();

    // Push 3 adhoc results
    for i in 1..=3 {
        state
            .query
            .push_history(Arc::new(adhoc_result(now, &format!("SELECT {}", i))));
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
    let (mut state, now) = explorer_selected_state();
    let mut terminal = create_test_terminal();

    let long_query = "SELECT column_1, column_2, column_3, column_4, column_5 FROM very_long_table_name WHERE id > 100";
    for i in 1..=3 {
        state
            .query
            .push_history(Arc::new(wide_adhoc_result(now, &format!("SELECT {}", i))));
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
    let (mut state, now) = explorer_selected_state();
    let mut terminal = create_test_terminal();

    // Push history but do NOT enter history mode (history_index = None)
    for i in 1..=2 {
        state
            .query
            .push_history(Arc::new(adhoc_result(now, &format!("SELECT {}", i))));
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
    let (mut state, now) = explorer_selected_state();
    let mut terminal = create_test_terminal();

    for i in 1..=3 {
        state
            .query
            .push_history(Arc::new(adhoc_result(now, &format!("SELECT {}", i))));
    }
    state
        .query
        .set_current_result(Arc::new(adhoc_result(now, "SELECT 3")));
    state.query.enter_history(0); // viewing 1/3
    state.ui.focus_mode = true;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
