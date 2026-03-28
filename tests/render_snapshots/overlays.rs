use super::*;
use harness::connected_state;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use sabiql::app::model::shared::multi_line_input::MultiLineInputState;

#[test]
fn sql_modal_with_completion() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor
        .set_content("SELECT * FROM us".to_string());
    state.sql_modal.set_status(SqlModalStatus::Editing);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_completion_popup_with_scroll() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.editor.set_content("SELECT ".to_string());
    state.sql_modal.set_status(SqlModalStatus::Editing);

    state.sql_modal.completion.visible = true;
    state.sql_modal.completion.selected_index = 5;
    state.sql_modal.completion.candidates = vec![
        CompletionCandidate {
            text: "users".into(),
            kind: CompletionKind::Table,
            score: 100,
        },
        CompletionCandidate {
            text: "posts".into(),
            kind: CompletionKind::Table,
            score: 90,
        },
        CompletionCandidate {
            text: "comments".into(),
            kind: CompletionKind::Table,
            score: 80,
        },
        CompletionCandidate {
            text: "id".into(),
            kind: CompletionKind::Column,
            score: 70,
        },
        CompletionCandidate {
            text: "name".into(),
            kind: CompletionKind::Column,
            score: 60,
        },
        CompletionCandidate {
            text: "email".into(),
            kind: CompletionKind::Column,
            score: 50,
        },
        CompletionCandidate {
            text: "created_at".into(),
            kind: CompletionKind::Column,
            score: 40,
        },
        CompletionCandidate {
            text: "updated_at".into(),
            kind: CompletionKind::Column,
            score: 30,
        },
        CompletionCandidate {
            text: "COUNT".into(),
            kind: CompletionKind::Keyword,
            score: 20,
        },
        CompletionCandidate {
            text: "DISTINCT".into(),
            kind: CompletionKind::Keyword,
            score: 10,
        },
    ];

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_cursor_at_head() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.editor = MultiLineInputState::new("SELECT 1", 0);
    state.sql_modal.set_status(SqlModalStatus::Editing);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_cursor_at_middle() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.editor = MultiLineInputState::new("SELECT 1", 4);
    state.sql_modal.set_status(SqlModalStatus::Editing);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_cursor_at_tail() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.editor.set_content("SELECT 1".to_string());
    state.sql_modal.set_status(SqlModalStatus::Editing);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_success_select() {
    let now = test_instant();
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor
        .set_content("SELECT * FROM users".to_string());
    state.sql_modal.mark_adhoc_success(AdhocSuccessSnapshot {
        command_tag: None,
        row_count: 2,
        execution_time_ms: 15,
    });
    state
        .query
        .set_current_result(Arc::new(sabiql::domain::QueryResult {
            query: "SELECT * FROM users".to_string(),
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![
                vec!["1".to_string(), "Alice".to_string()],
                vec!["2".to_string(), "Bob".to_string()],
            ],
            row_count: 2,
            execution_time_ms: 15,
            executed_at: now,
            source: QuerySource::Adhoc,
            error: None,
            command_tag: None,
        }));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_success_dml_with_command_tag() {
    let now = test_instant();
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor
        .set_content("DELETE FROM users WHERE id = 1".to_string());
    state.sql_modal.mark_adhoc_success(AdhocSuccessSnapshot {
        command_tag: Some(CommandTag::Delete(3)),
        row_count: 3,
        execution_time_ms: 12,
    });
    state
        .query
        .set_current_result(Arc::new(sabiql::domain::QueryResult {
            query: "DELETE FROM users WHERE id = 1".to_string(),
            columns: vec![],
            rows: vec![],
            row_count: 3,
            execution_time_ms: 12,
            executed_at: now,
            source: QuerySource::Adhoc,
            error: None,
            command_tag: Some(CommandTag::Delete(3)),
        }));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_success_ddl_create_table() {
    let now = test_instant();
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor
        .set_content("CREATE TABLE backup AS SELECT * FROM users".to_string());
    state.sql_modal.mark_adhoc_success(AdhocSuccessSnapshot {
        command_tag: Some(CommandTag::Create("TABLE".to_string())),
        row_count: 0,
        execution_time_ms: 45,
    });
    state
        .query
        .set_current_result(Arc::new(sabiql::domain::QueryResult {
            query: "CREATE TABLE backup AS SELECT * FROM users".to_string(),
            columns: vec![],
            rows: vec![],
            row_count: 0,
            execution_time_ms: 45,
            executed_at: now,
            source: QuerySource::Adhoc,
            error: None,
            command_tag: Some(CommandTag::Create("TABLE".to_string())),
        }));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_error_with_message() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor
        .set_content("SELECT * FORM users".to_string());
    state.sql_modal.mark_adhoc_error(
        "ERROR:  syntax error at or near \"FORM\"\nLINE 1: SELECT * FORM users\n                 ^"
            .to_string(),
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_confirming_high_matched() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor
        .set_content("DROP TABLE users".to_string());
    let mut input = TextInputState::default();
    input.set_content("users".to_string());
    state.sql_modal.set_status(SqlModalStatus::ConfirmingHigh {
        decision: AdhocRiskDecision {
            risk_level: RiskLevel::High,
            label: "DROP",
        },
        input,
        target_name: Some("users".to_string()),
    });

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_confirming_high_unmatched() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor
        .set_content("DROP TABLE users".to_string());
    let mut input = TextInputState::default();
    input.set_content("use".to_string());
    state.sql_modal.set_status(SqlModalStatus::ConfirmingHigh {
        decision: AdhocRiskDecision {
            risk_level: RiskLevel::High,
            label: "DROP",
        },
        input,
        target_name: Some("users".to_string()),
    });

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_confirming_high_no_target() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor
        .set_content("DROP TABLE users".to_string());
    state.sql_modal.set_status(SqlModalStatus::ConfirmingHigh {
        decision: AdhocRiskDecision {
            risk_level: RiskLevel::High,
            label: "DROP",
        },
        input: TextInputState::default(),
        target_name: None,
    });

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn help_overlay() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::Help);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn command_palette_overlay() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::CommandPalette);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn table_picker_overlay() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::TablePicker);
    state
        .ui
        .table_picker
        .filter_input
        .set_content("user".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn command_line_input() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::CommandLine);
    state.command_line_input.set_content("sql".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn query_history_picker_with_entries() {
    use sabiql::domain::ConnectionId;
    use sabiql::domain::query_history::{QueryHistoryEntry, QueryResultStatus};

    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::QueryHistoryPicker);
    state.query_history_picker.entries = vec![
        QueryHistoryEntry::new(
            "SELECT * FROM users WHERE id = 1".to_string(),
            "2026-03-13T10:00:00Z".to_string(),
            ConnectionId::from_string("test-conn"),
            QueryResultStatus::Success,
            None,
        ),
        QueryHistoryEntry::new(
            "INSERT INTO orders (user_id, total) VALUES (1, 100)".to_string(),
            "2026-03-13T11:00:00Z".to_string(),
            ConnectionId::from_string("test-conn"),
            QueryResultStatus::Success,
            Some(1),
        ),
        QueryHistoryEntry::new(
            "SELECT count(*) FROM users".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-conn"),
            QueryResultStatus::Failed,
            None,
        ),
    ];

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn query_history_picker_empty() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::QueryHistoryPicker);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn query_history_picker_filter_mode() {
    use sabiql::domain::ConnectionId;
    use sabiql::domain::query_history::{QueryHistoryEntry, QueryResultStatus};

    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::QueryHistoryPicker);
    state.query_history_picker.entries = vec![
        QueryHistoryEntry::new(
            "SELECT * FROM users".to_string(),
            "2026-03-13T10:00:00Z".to_string(),
            ConnectionId::from_string("test-conn"),
            QueryResultStatus::Success,
            None,
        ),
        QueryHistoryEntry::new(
            "SELECT * FROM orders".to_string(),
            "2026-03-13T11:00:00Z".to_string(),
            ConnectionId::from_string("test-conn"),
            QueryResultStatus::Success,
            None,
        ),
    ];
    state.query_history_picker.filter_input.insert_str("user");

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_plan_tab_placeholder() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.active_tab = SqlModalTab::Plan;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_plan_tab_with_plan_text() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.active_tab = SqlModalTab::Plan;
    state.explain.set_plan(
        "Seq Scan on users  (cost=0.00..35.50 rows=2550 width=36)\n  Filter: (id > 10)".to_string(),
        false,
        42,
        "SELECT * FROM users WHERE id > 10",
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_plan_tab_with_error() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.active_tab = SqlModalTab::Plan;
    state
        .explain
        .set_error("ERROR: relation \"nonexistent\" does not exist".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_compare_tab_empty() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.active_tab = SqlModalTab::Compare;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_compare_tab_right_only() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state.explain.set_plan(
        "Seq Scan on users  (cost=0.00..10.20 rows=10 width=3273)\n  Filter: email_verified"
            .to_string(),
        false,
        40,
        "SELECT * FROM users WHERE email_verified",
    );
    // Only right slot populated (first EXPLAIN), no left yet
    state.sql_modal.active_tab = SqlModalTab::Compare;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_compare_tab_with_verdict() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    // First EXPLAIN with high cost
    state.explain.set_plan(
        "Seq Scan on users  (cost=0.00..1000.00 rows=2550 width=36)\n  Filter: (id > 10)"
            .to_string(),
        false,
        100,
        "SELECT * FROM users WHERE id > 10",
    );
    // Second EXPLAIN with low cost (Improved) — auto-advances first to left
    state.explain.set_plan(
        "Index Scan using idx_users_id on users  (cost=0.28..8.30 rows=1 width=36)\n  Index Cond: (id > 10)"
            .to_string(),
        false,
        5,
        "SELECT * FROM users WHERE id > 10",
    );
    state.sql_modal.active_tab = SqlModalTab::Compare;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_compare_tab_unavailable() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    // First EXPLAIN with unparseable text
    state.explain.set_plan(
        "CREATE TABLE foo (id int)".to_string(),
        false,
        0,
        "CREATE TABLE foo",
    );
    // Second EXPLAIN with also unparseable text — auto-advances first to left
    state.explain.set_plan(
        "ALTER TABLE foo ADD COLUMN bar text".to_string(),
        false,
        0,
        "ALTER TABLE foo",
    );
    state.sql_modal.active_tab = SqlModalTab::Compare;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_compare_tab_narrow_stacked() {
    let mut state = create_test_state();
    let backend = TestBackend::new(50, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    state.modal.set_mode(InputMode::SqlModal);
    // First EXPLAIN
    state.explain.set_plan(
        "Seq Scan on users  (cost=0.00..1000.00 rows=2550 width=36)\n  Filter: (id > 10)"
            .to_string(),
        false,
        100,
        "SELECT * FROM users WHERE id > 10",
    );
    // Second EXPLAIN — auto-advances first to left
    state.explain.set_plan(
        "Index Scan using idx_users_id on users  (cost=0.28..8.30 rows=1 width=36)\n  Index Cond: (id > 10)"
            .to_string(),
        false,
        5,
        "SELECT * FROM users WHERE id > 10",
    );
    state.sql_modal.active_tab = SqlModalTab::Compare;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sql_modal_normal_initial() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    // Normal mode is the default — empty editor with placeholder

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
