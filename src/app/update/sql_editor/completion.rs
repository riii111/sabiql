use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::policy::sql::completion::{CompletionEngine, CompletionInput};
use crate::update::browse::metadata::prefetch::{prefetch_table_detail, start_completion_prefetch};

pub(in crate::update) fn completion_request(state: &mut AppState, now: Instant) -> Vec<Effect> {
    let outcome = CompletionEngine::new(&state.table_detail_cache).complete(CompletionInput {
        content: state.sql_modal.editor().content(),
        cursor: state.sql_modal.editor().cursor(),
        database_type: state.session.active_database_type_or_default(),
        active_database: state.session.active_database(),
        metadata: state.session.metadata(),
        table_detail: state.session.table_detail(),
    });
    state.sql_modal.apply_completion_update(
        &outcome.candidates,
        outcome.trigger_position,
        outcome.visible,
    );
    prefetch_missing_tables(state, outcome.missing_tables, now)
}

fn prefetch_missing_tables(
    state: &mut AppState,
    missing_tables: Vec<String>,
    now: Instant,
) -> Vec<Effect> {
    let Some(run_id) = state.table_prefetch.active_prefetch_run_id() else {
        return start_completion_prefetch(state, missing_tables);
    };
    let mut effects = Vec::new();
    for qualified_name in missing_tables {
        if let Some((schema, table)) = qualified_name.split_once('.') {
            effects.extend(prefetch_table_detail(state, run_id, schema, table, now));
        }
    }
    effects
}

#[cfg(test)]
mod tests {
    use crate::update::action::Action;
    use std::sync::Arc;

    use super::*;
    use crate::domain::{DatabaseMetadata, TableSummary};
    use crate::model::shared::input_mode::InputMode;
    use crate::update::reducer::route;
    use crate::update::test_fixtures;

    #[test]
    fn request_prefetches_only_referenced_tables() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::SqlModal);
        state
            .sql_modal
            .editor
            .set_content("SELECT * FROM public.users".to_string());
        let mut metadata = DatabaseMetadata::new("test".to_string());
        metadata.table_summaries = (0..3)
            .map(|index| TableSummary::new("public".to_string(), format!("table_{index}"), None))
            .collect();
        state.session.set_metadata(Some(Arc::new(metadata)));

        let effects = route(&mut state, Action::CompletionRequest, Instant::now());

        assert!(matches!(
            effects.as_slice(),
            [Effect::SchedulePrefetchQueueProcessing { .. }]
        ));
        assert_eq!(state.table_prefetch.pending_prefetch_count(), 1);
    }

    #[test]
    fn request_during_active_prefetch_fetches_missing_tables_in_that_run() {
        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/test");
        state.modal.set_mode(InputMode::SqlModal);
        state
            .sql_modal
            .editor
            .set_content("SELECT * FROM public.users".to_string());
        let run_id = state.table_prefetch.begin_completion_prefetch();

        let effects = route(&mut state, Action::CompletionRequest, Instant::now());

        assert!(matches!(
            effects.as_slice(),
            [Effect::PrefetchTableColumnsAndFks { run_id: fetch_run_id, schema, table, .. }]
                if *fetch_run_id == run_id && schema == "public" && table == "users"
        ));
    }
}
