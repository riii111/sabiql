use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::sql_editor::modal::sql_modal_visible_rows;
use crate::policy::sql::completion::{CompletionEngine, CompletionInput};
use crate::update::action::Action;

pub(in crate::update) fn completion_next(state: &mut AppState) -> Vec<Effect> {
    state.sql_modal.completion_next();
    vec![]
}

pub(in crate::update) fn completion_prev(state: &mut AppState) -> Vec<Effect> {
    state.sql_modal.completion_prev();
    vec![]
}

pub(in crate::update) fn completion_dismiss(state: &mut AppState) -> Vec<Effect> {
    state.sql_modal.dismiss_completion();
    vec![]
}

pub(in crate::update) fn completion_accept(state: &mut AppState) -> Vec<Effect> {
    state
        .sql_modal
        .accept_selected_completion(sql_modal_visible_rows(state.ui.terminal_height()));
    vec![]
}

pub(in crate::update) fn completion_request(state: &mut AppState) -> Vec<Effect> {
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
    prefetch_missing_tables(state, outcome.missing_tables)
}

fn prefetch_missing_tables(state: &AppState, missing_tables: Vec<String>) -> Vec<Effect> {
    if missing_tables.is_empty() {
        return vec![];
    }
    let actions: Vec<Action> = match state.table_prefetch.active_prefetch_run_id() {
        Some(run_id) => missing_tables
            .into_iter()
            .filter_map(|qualified_name| {
                qualified_name
                    .split_once('.')
                    .map(|(schema, table)| Action::PrefetchTableDetail {
                        run_id,
                        schema: schema.to_string(),
                        table: table.to_string(),
                    })
            })
            .collect(),
        None => vec![Action::StartCompletionPrefetch {
            tables: missing_tables,
        }],
    };
    if actions.is_empty() {
        return vec![];
    }
    vec![Effect::DispatchActions(actions)]
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::domain::{DatabaseMetadata, TableSummary};
    use crate::model::shared::input_mode::InputMode;
    use crate::services::AppServices;
    use crate::update::reducer::route;
    use std::time::Instant;

    #[test]
    fn request_prefetches_only_referenced_tables() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::SqlModal);
        state
            .sql_modal
            .editor_mut_for_input()
            .set_content("SELECT * FROM public.users".to_string());
        let mut metadata = DatabaseMetadata::new("test".to_string());
        metadata.table_summaries = (0..1_000)
            .map(|index| {
                TableSummary::new("public".to_string(), format!("table_{index}"), None, false)
            })
            .collect();
        state.session.set_metadata(Some(Arc::new(metadata)));

        let effects = route(
            &mut state,
            Action::CompletionRequest,
            Instant::now(),
            &AppServices::stub(),
        );

        assert!(matches!(
            effects.as_slice(),
            [Effect::DispatchActions(actions)]
                if matches!(
                    actions.as_slice(),
                    [Action::StartCompletionPrefetch { tables }]
                        if *tables == vec!["public.users".to_string()]
                )
        ));
    }
}
