use super::*;
use app::model::app_state::AppState;
use app::model::shared::confirm_dialog::ConfirmIntent;
use harness::connected_state;

fn make_update_preview(diff: Vec<ColumnDiff>, sql: String) -> WritePreview {
    make_update_preview_with_key(diff, sql, "1")
}

fn make_update_preview_with_key(diff: Vec<ColumnDiff>, sql: String, id: &str) -> WritePreview {
    WritePreview {
        operation: WriteOperation::Update,
        sql,
        target_summary: TargetSummary {
            schema: "public".to_string(),
            table: "users".to_string(),
            key_values: vec![("id".to_string(), id.to_string())],
        },
        diff,
        guardrail: GuardrailDecision {
            risk_level: RiskLevel::Low,
            blocked: false,
            reason: None,
            target_summary: None,
        },
    }
}

fn open_write_confirm(state: &mut AppState, title: &str, sql: &str) {
    state.modal.set_mode(InputMode::ConfirmDialog);
    state.confirm_dialog.open(
        title,
        "",
        ConfirmIntent::ExecuteWrite {
            sql: sql.to_string(),
            blocked: false,
        },
    );
}

#[test]
fn confirm_dialog() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConfirmDialog);
    state.confirm_dialog.open(
        "Confirm",
        "No connection configured.\nAre you sure you want to quit?",
        ConfirmIntent::QuitNoConnection,
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn confirm_dialog_update_preview() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);
    state.modal.set_mode(InputMode::ConfirmDialog);
    state.confirm_dialog.open(
        "Confirm UPDATE: users",
        "email: \"bob@example.com\" -> \"new@example.com\"\n\nUPDATE \"public\".\"users\"\nSET \"email\" = 'new@example.com'\nWHERE \"id\" = '2';",
        ConfirmIntent::ExecuteWrite {
            sql: "UPDATE \"public\".\"users\"\nSET \"email\" = 'new@example.com'\nWHERE \"id\" = '2';".to_string(),
            blocked: false,
        },
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn confirm_dialog_update_preview_rich() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);

    let sql = "UPDATE \"public\".\"users\"\nSET \"email\" = 'new@example.com'\nWHERE \"id\" = '2';"
        .to_string();
    state
        .result_interaction
        .set_write_preview(make_update_preview_with_key(
            vec![ColumnDiff {
                column: "email".to_string(),
                before: "bob@example.com".to_string(),
                after: "new@example.com".to_string(),
                json_diff: None,
            }],
            sql.clone(),
            "2",
        ));
    state.modal.set_mode(InputMode::ConfirmDialog);
    state.confirm_dialog.open(
        "Confirm UPDATE: users",
        "email: \"bob@example.com\" -> \"new@example.com\"",
        ConfirmIntent::ExecuteWrite {
            sql,
            blocked: false,
        },
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn confirm_dialog_delete_preview_low_risk() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);

    let sql = "DELETE FROM \"public\".\"users\"\nWHERE \"id\" = '3';".to_string();
    state.result_interaction.set_write_preview(WritePreview {
        operation: WriteOperation::Delete,
        sql: sql.clone(),
        target_summary: TargetSummary {
            schema: "public".to_string(),
            table: "users".to_string(),
            key_values: vec![("id".to_string(), "3".to_string())],
        },
        diff: vec![],
        guardrail: GuardrailDecision {
            risk_level: RiskLevel::Low,
            blocked: false,
            reason: None,
            target_summary: None,
        },
    });
    open_write_confirm(&mut state, "Confirm DELETE: users", &sql);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn confirm_dialog_update_preview_long_jsonb() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);

    let long_before = r#"{"industries": ["tech", "finance", "healthcare"], "company_size": "enterprise", "preferences": {"notifications": true, "theme": "dark"}}"#;
    let long_after = r#"{"industries": ["tech", "retail"], "company_size": "startup", "preferences": {"notifications": false, "theme": "light", "language": "ja"}}"#;

    let sql = format!(
        "UPDATE \"public\".\"users\"\nSET \"metadata\" = '{long_after}'\nWHERE \"id\" = '1';"
    );
    let json_diff = compute_json_diff(long_before, long_after, 1);
    assert!(
        json_diff.is_some(),
        "expected structured JSON diff for long_jsonb snapshot"
    );
    state
        .result_interaction
        .set_write_preview(make_update_preview(
            vec![ColumnDiff {
                column: "metadata".to_string(),
                before: long_before.to_string(),
                after: long_after.to_string(),
                json_diff,
            }],
            sql.clone(),
        ));
    open_write_confirm(&mut state, "Confirm UPDATE: users", &sql);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn confirm_dialog_update_preview_jsonb_key_order_normalized() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);

    // before: PostgreSQL key order (industries first, spaces)
    // after: serde_json key order (alphabetical, compact)
    // Only actual change is company_size value: "500+" → "600+"
    let pg_before =
        r#"{"industries": ["technology", "finance"], "company_size": ["100-500", "500+"]}"#;
    let serde_after =
        r#"{"company_size":["100-500","600+"],"industries":["technology","finance"]}"#;

    let sql = format!(
        "UPDATE \"public\".\"users\"\nSET \"target_audience\" = '{serde_after}'\nWHERE \"id\" = '1';"
    );
    // Apply normalize_for_diff to mirror the real build_update_preview path
    let before = normalize_for_diff(pg_before);
    let after = normalize_for_diff(serde_after);
    let json_diff = compute_json_diff(&before, &after, 1);
    assert!(
        json_diff.is_some(),
        "expected structured JSON diff for key_order_normalized snapshot"
    );
    state
        .result_interaction
        .set_write_preview(make_update_preview(
            vec![ColumnDiff {
                column: "target_audience".to_string(),
                before,
                after,
                json_diff,
            }],
            sql.clone(),
        ));
    open_write_confirm(&mut state, "Confirm UPDATE: users", &sql);

    let output = render_to_string(&mut terminal, &mut state);

    // Both before and after should show the same key order after normalization
    // (serde_json alphabetical: company_size before industries)
    insta::assert_snapshot!(output);
}

#[test]
fn confirm_dialog_update_preview_scrollable() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);

    let sql = "UPDATE \"public\".\"users\"\nSET \"a\" = '1', \"b\" = '2', \"c\" = '3', \"d\" = '4', \"e\" = '5'\nWHERE \"id\" = '1';".to_string();
    state
        .result_interaction
        .set_write_preview(make_update_preview(
            vec![
                ColumnDiff {
                    column: "a".to_string(),
                    before: "old_a".to_string(),
                    after: "new_a".to_string(),
                    json_diff: None,
                },
                ColumnDiff {
                    column: "b".to_string(),
                    before: "old_b".to_string(),
                    after: "new_b".to_string(),
                    json_diff: None,
                },
                ColumnDiff {
                    column: "c".to_string(),
                    before: "old_c".to_string(),
                    after: "new_c".to_string(),
                    json_diff: None,
                },
                ColumnDiff {
                    column: "d".to_string(),
                    before: "old_d".to_string(),
                    after: "new_d".to_string(),
                    json_diff: None,
                },
                ColumnDiff {
                    column: "e".to_string(),
                    before: "old_e".to_string(),
                    after: "new_e".to_string(),
                    json_diff: None,
                },
            ],
            sql.clone(),
        ));
    open_write_confirm(&mut state, "Confirm UPDATE: users", &sql);

    let output = render_to_string(&mut terminal, &mut state);

    assert!(
        output.contains("Enter: Confirm │ Esc: Cancel"),
        "Scrollable preview should keep only primary actions in the hint"
    );
    insta::assert_snapshot!(output);
}

#[test]
fn confirm_dialog_update_preview_narrow_terminal() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal_sized(40, 12);

    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);

    let long_before = r#"{"industries": ["tech", "finance"], "company_size": "enterprise"}"#;
    let long_after = r#"{"industries": ["tech"], "company_size": "startup"}"#;

    let sql = format!(
        "UPDATE \"public\".\"users\"\nSET \"metadata\" = '{long_after}'\nWHERE \"id\" = '1';"
    );
    let json_diff = compute_json_diff(long_before, long_after, 1);
    assert!(
        json_diff.is_some(),
        "expected structured JSON diff for narrow_terminal snapshot"
    );
    state
        .result_interaction
        .set_write_preview(make_update_preview(
            vec![ColumnDiff {
                column: "metadata".to_string(),
                before: long_before.to_string(),
                after: long_after.to_string(),
                json_diff,
            }],
            sql.clone(),
        ));
    open_write_confirm(&mut state, "Confirm UPDATE: users", &sql);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn confirm_dialog_update_preview_multi_column() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);

    let sql = "UPDATE \"public\".\"users\"\nSET \"email\" = 'new@example.com', \"name\" = 'New Name'\nWHERE \"id\" = '2';".to_string();
    state
        .result_interaction
        .set_write_preview(make_update_preview_with_key(
            vec![
                ColumnDiff {
                    column: "email".to_string(),
                    before: "bob@example.com".to_string(),
                    after: "new@example.com".to_string(),
                    json_diff: None,
                },
                ColumnDiff {
                    column: "name".to_string(),
                    before: "Bob".to_string(),
                    after: "New Name".to_string(),
                    json_diff: None,
                },
            ],
            sql.clone(),
            "2",
        ));
    open_write_confirm(&mut state, "Confirm UPDATE: users", &sql);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn confirm_dialog_update_preview_jsonb_structured_diff_with_ellipsis() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);

    // Large nested JSON where only one deep value changes, forcing ellipsis
    let before = r#"{"alpha": 1, "beta": 2, "gamma": 3, "delta": 4, "epsilon": 5, "zeta": {"nested_a": "unchanged", "nested_b": "old_value", "nested_c": "unchanged"}, "eta": 7, "theta": 8}"#;
    let after = r#"{"alpha": 1, "beta": 2, "gamma": 3, "delta": 4, "epsilon": 5, "zeta": {"nested_a": "unchanged", "nested_b": "new_value", "nested_c": "unchanged"}, "eta": 7, "theta": 8}"#;

    let sql =
        format!("UPDATE \"public\".\"users\"\nSET \"config\" = '{after}'\nWHERE \"id\" = '1';");
    let json_diff = compute_json_diff(before, after, 1);
    assert!(
        json_diff.is_some(),
        "expected structured JSON diff for ellipsis snapshot"
    );
    state
        .result_interaction
        .set_write_preview(make_update_preview(
            vec![ColumnDiff {
                column: "config".to_string(),
                before: before.to_string(),
                after: after.to_string(),
                json_diff,
            }],
            sql.clone(),
        ));
    open_write_confirm(&mut state, "Confirm UPDATE: users", &sql);

    let output = render_to_string(&mut terminal, &mut state);

    assert!(
        output.contains("..."),
        "large JSON diff should contain ellipsis"
    );
    insta::assert_snapshot!(output);
}

#[test]
fn confirm_dialog_update_preview_jsonb_and_string_mixed() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);

    let json_before = r#"{"status": "active", "role": "admin"}"#;
    let json_after = r#"{"status": "inactive", "role": "admin"}"#;
    let json_diff = compute_json_diff(json_before, json_after, 1);
    assert!(
        json_diff.is_some(),
        "expected structured JSON diff for mixed snapshot"
    );

    let sql = "UPDATE \"public\".\"users\"\nSET \"metadata\" = '{...}', \"email\" = 'new@example.com'\nWHERE \"id\" = '1';".to_string();
    state
        .result_interaction
        .set_write_preview(make_update_preview(
            vec![
                ColumnDiff {
                    column: "metadata".to_string(),
                    before: json_before.to_string(),
                    after: json_after.to_string(),
                    json_diff,
                },
                ColumnDiff {
                    column: "email".to_string(),
                    before: "old@example.com".to_string(),
                    after: "new@example.com".to_string(),
                    json_diff: None,
                },
            ],
            sql.clone(),
        ));
    open_write_confirm(&mut state, "Confirm UPDATE: users", &sql);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
