use super::*;
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

fn open_write_confirm(state: &mut sabiql::app::model::app_state::AppState, title: &str, sql: &str) {
    state.modal.set_mode(InputMode::ConfirmDialog);
    state.confirm_dialog.open(
        title,
        "",
        sabiql::app::model::shared::confirm_dialog::ConfirmIntent::ExecuteWrite {
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
        sabiql::app::model::shared::confirm_dialog::ConfirmIntent::QuitNoConnection,
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
        sabiql::app::model::shared::confirm_dialog::ConfirmIntent::ExecuteWrite {
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
            }],
            sql.clone(),
            "2",
        ));
    state.modal.set_mode(InputMode::ConfirmDialog);
    state.confirm_dialog.open(
        "Confirm UPDATE: users",
        "email: \"bob@example.com\" -> \"new@example.com\"",
        sabiql::app::model::shared::confirm_dialog::ConfirmIntent::ExecuteWrite {
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
    state
        .result_interaction
        .set_write_preview(make_update_preview(
            vec![ColumnDiff {
                column: "metadata".to_string(),
                before: long_before.to_string(),
                after: long_after.to_string(),
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
    state
        .result_interaction
        .set_write_preview(make_update_preview(
            vec![ColumnDiff {
                column: "target_audience".to_string(),
                before: normalize_for_diff(pg_before),
                after: normalize_for_diff(serde_after),
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
                },
                ColumnDiff {
                    column: "b".to_string(),
                    before: "old_b".to_string(),
                    after: "new_b".to_string(),
                },
                ColumnDiff {
                    column: "c".to_string(),
                    before: "old_c".to_string(),
                    after: "new_c".to_string(),
                },
                ColumnDiff {
                    column: "d".to_string(),
                    before: "old_d".to_string(),
                    after: "new_d".to_string(),
                },
                ColumnDiff {
                    column: "e".to_string(),
                    before: "old_e".to_string(),
                    after: "new_e".to_string(),
                },
            ],
            sql.clone(),
        ));
    open_write_confirm(&mut state, "Confirm UPDATE: users", &sql);

    let output = render_to_string(&mut terminal, &mut state);

    assert!(
        output.contains("Scroll"),
        "Scrollable preview should show scroll hint"
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
    state
        .result_interaction
        .set_write_preview(make_update_preview(
            vec![ColumnDiff {
                column: "metadata".to_string(),
                before: long_before.to_string(),
                after: long_after.to_string(),
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
                },
                ColumnDiff {
                    column: "name".to_string(),
                    before: "Bob".to_string(),
                    after: "New Name".to_string(),
                },
            ],
            sql.clone(),
            "2",
        ));
    open_write_confirm(&mut state, "Confirm UPDATE: users", &sql);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
