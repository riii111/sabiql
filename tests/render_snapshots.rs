mod harness;

use harness::fixtures;
use harness::{create_test_state, create_test_terminal, render_to_string, test_instant};

use std::time::Duration;

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
fn table_selection_with_preview() {
    let now = test_instant();
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.cache.metadata = Some(fixtures::sample_metadata(now));
    state.cache.state = MetadataState::Loaded;
    state.ui.set_explorer_selection(Some(0));
    state.cache.table_detail = Some(fixtures::sample_table_detail());
    state.query.current_result = Some(fixtures::sample_query_result(now));

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
    state.query.current_result = Some(fixtures::sample_query_result(now));
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
    state.query.current_result = Some(fixtures::sample_query_result(now));
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
    state.query.current_result = Some(fixtures::empty_query_result(now));

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
