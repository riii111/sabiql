use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::policy::sql::completion::{CompletionDatabaseScope, CompletionEngine};
use crate::update::action::Action;

pub(in crate::cmd) fn run(effect: Effect, state: &AppState) -> Vec<Action> {
    match effect {
        Effect::TriggerCompletion => {
            let cursor = state.sql_modal.editor().cursor();
            let content = state.sql_modal.editor().content();
            let database_type = state.session.active_database_type_or_default();
            let active_database = state.session.active_database();

            let engine = CompletionEngine::new(&state.table_detail_cache);
            let prep = engine.prepare_for_database(content, cursor, database_type);
            let missing = engine.missing_tables_prepared(&prep, state.session.metadata());

            let mut actions = Vec::new();
            if !missing.is_empty() {
                if let Some(run_id) = state.table_prefetch.active_prefetch_run_id() {
                    for action in missing.into_iter().filter_map(|qualified_name| {
                        qualified_name.split_once('.').map(|(schema, table)| {
                            Action::PrefetchTableDetail {
                                run_id,
                                schema: schema.to_string(),
                                table: table.to_string(),
                            }
                        })
                    }) {
                        actions.push(action);
                    }
                } else {
                    actions.push(Action::StartCompletionPrefetch { tables: missing });
                }
            }

            let token_len = CompletionEngine::current_token_len_prepared(&prep);
            let candidates = engine.get_candidates_prepared_for_database(
                content,
                cursor,
                &prep,
                state.session.metadata(),
                state.session.table_detail(),
                CompletionDatabaseScope {
                    database_type,
                    active_database,
                },
            );
            let visible = !candidates.is_empty() && !content.trim().is_empty();

            actions.push(Action::CompletionUpdated {
                candidates,
                trigger_position: cursor.saturating_sub(token_len),
                visible,
                dsn: state.session.dsn().map(str::to_string),
                connection_generation: state.session.connection_generation(),
                database_generation: state.session.database_generation(),
                metadata_generation: state.session.metadata_generation(),
            });
            actions
        }

        _ => unreachable!("completion::run called with non-completion effect"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DatabaseMetadata, TableSummary};
    use crate::model::shared::input_mode::InputMode;
    use std::sync::Arc;

    #[test]
    fn trigger_completion_prefetches_only_referenced_tables() {
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

        let actions = run(Effect::TriggerCompletion, &state);

        assert!(matches!(
            actions.as_slice(),
            [Action::StartCompletionPrefetch { tables }, Action::CompletionUpdated { .. }]
                if *tables == vec!["public.users".to_string()]
        ));
    }
}
