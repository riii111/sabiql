use crate::cmd::effect::Effect;
use crate::domain::{ConnectionId, DatabaseType, QueryResult, QuerySource, QueryValue};
use crate::model::app_state::AppState;
use crate::model::browse::query_execution::PostDeleteRowSelection;
use crate::model::shared::confirm_dialog::ConfirmIntent;
use crate::model::shared::input_mode::InputMode;
use crate::policy::write::write_guardrails::{
    GuardrailDecision, RiskLevel, TargetSummary, WriteOperation, WritePreview,
};
use crate::services::AppServices;
use crate::update::action::{Action, QueryCompletionContext, TableTarget};
use crate::update::reducer::reduce;
use std::sync::Arc;
use std::time::Instant;

pub fn activate_postgres_connection(state: &mut AppState, dsn: &str) {
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "postgres",
        DatabaseType::PostgreSQL,
        dsn,
    );
}

pub fn activate_sqlite_connection(state: &mut AppState, dsn: &str) {
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "sqlite",
        DatabaseType::SQLite,
        dsn,
    );
}

pub fn activate_mysql_connection(state: &mut AppState, dsn: &str) {
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "mysql",
        DatabaseType::MySQL,
        dsn,
    );
}

pub fn state_after_delete_success() -> AppState {
    let mut state = AppState::new("test_project".to_string());
    activate_postgres_connection(&mut state, "postgres://localhost/test");
    state.modal.set_mode(InputMode::ConfirmDialog);
    state.result_interaction.set_write_preview(WritePreview {
        operation: WriteOperation::Delete,
        sql: "DELETE FROM \"public\".\"users\" WHERE \"id\" = '2';".to_string(),
        target_summary: TargetSummary {
            schema: "public".to_string(),
            table: "users".to_string(),
            key_values: vec![("id".to_string(), QueryValue::text("2"))],
        },
        diff: vec![],
        guardrail: GuardrailDecision {
            risk_level: RiskLevel::Low,
            blocked: false,
            reason: None,
            target_summary: None,
        },
    });
    state.query.set_delete_refresh_target(0, Some(499), 1);
    state.confirm_dialog.open(
        "",
        "",
        ConfirmIntent::ExecuteWrite {
            sql: "DELETE FROM \"public\".\"users\" WHERE \"id\" = '2';".to_string(),
            blocked: false,
        },
    );

    let now = Instant::now();
    let effects = reduce(
        &mut state,
        Action::ConfirmDialogConfirm,
        now,
        &AppServices::stub(),
    );
    let run_id = match effects.as_slice() {
        [Effect::ExecuteWrite { run_id, .. }] => *run_id,
        other => panic!("expected ExecuteWrite, got {other:?}"),
    };
    let effects = reduce(
        &mut state,
        Action::ExecuteWriteSucceeded {
            dsn: "postgres://localhost/test".to_string(),
            run_id,
            affected_rows: 1,
            diagnostics: Vec::new(),
        },
        now,
        &AppServices::stub(),
    );

    assert!(matches!(
        effects.as_slice(),
        [Effect::ExecutePreview { .. }]
    ));
    assert_eq!(
        state.query.post_delete_row_selection(),
        PostDeleteRowSelection::Select(499)
    );
    state
}

pub fn complete_table_preview(state: &mut AppState, now: Instant) {
    let generation = state.session.selection_generation();
    let effects = reduce(
        state,
        Action::ExecutePreview(TableTarget {
            schema: "public".to_string(),
            table: "users".to_string(),
            generation,
        }),
        now,
        &AppServices::stub(),
    );
    let run_id = match effects.as_slice() {
        [Effect::ExecutePreview { run_id, .. }] => *run_id,
        other => panic!("expected ExecutePreview, got {other:?}"),
    };
    reduce(
        state,
        Action::QueryCompleted {
            run_id,
            result: Arc::new(QueryResult::success(
                "SELECT * FROM users".to_string(),
                vec!["id".to_string()],
                vec![vec!["1".to_string()]],
                1,
                QuerySource::Preview,
            )),
            context: QueryCompletionContext::Preview {
                generation,
                target_page: 0,
            },
        },
        now,
        &AppServices::stub(),
    );
}

pub fn assert_connection_save_fetch_effects(effects: &[Effect], database_type: DatabaseType) {
    match database_type {
        DatabaseType::SQLite => {
            assert_eq!(effects.len(), 1, "sqlite save should emit Sequence");
            let Effect::Sequence(seq) = &effects[0] else {
                panic!("expected Sequence, got {effects:?}");
            };
            assert_eq!(seq.len(), 4);
            assert!(matches!(seq[0], Effect::CancelTrackedTasks));
            assert!(matches!(seq[1], Effect::CacheInvalidate { .. }));
            assert!(matches!(seq[2], Effect::ClearCompletionEngineCache));
            assert!(matches!(seq[3], Effect::FetchMetadata { .. }));
        }
        DatabaseType::PostgreSQL => {
            assert_eq!(
                effects.len(),
                3,
                "postgres save should preserve prefetched metadata cache"
            );
            assert!(matches!(effects[0], Effect::CancelTrackedTasks));
            assert!(matches!(effects[1], Effect::ClearCompletionEngineCache));
            assert!(matches!(effects[2], Effect::FetchMetadata { .. }));
        }
        DatabaseType::MySQL => {
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::ClearCompletionEngineCache));
        }
    }
}
