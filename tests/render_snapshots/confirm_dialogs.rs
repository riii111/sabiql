use super::*;
use harness::connected_state;

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
    state.result_interaction.set_write_preview(WritePreview {
        operation: WriteOperation::Update,
        sql: sql.clone(),
        target_summary: TargetSummary {
            schema: "public".to_string(),
            table: "users".to_string(),
            key_values: vec![("id".to_string(), "2".to_string())],
        },
        diff: vec![ColumnDiff {
            column: "email".to_string(),
            before: "bob@example.com".to_string(),
            after: "new@example.com".to_string(),
        }],
        guardrail: GuardrailDecision {
            risk_level: RiskLevel::Low,
            blocked: false,
            reason: None,
            target_summary: None,
        },
    });
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
    state.modal.set_mode(InputMode::ConfirmDialog);
    state.confirm_dialog.open(
        "Confirm DELETE: users",
        "",
        sabiql::app::model::shared::confirm_dialog::ConfirmIntent::ExecuteWrite {
            sql,
            blocked: false,
        },
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
