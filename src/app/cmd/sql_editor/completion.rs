use std::cell::RefCell;

use crate::cmd::completion_engine::{CompletionDatabaseScope, CompletionEngine};
use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::text_input::TextInputLike;
use crate::update::action::Action;

pub fn run(
    effect: Effect,
    state: &AppState,
    completion_engine: &RefCell<CompletionEngine>,
) -> Vec<Action> {
    match effect {
        Effect::CacheTableInCompletionEngine {
            qualified_name,
            table,
        } => {
            completion_engine
                .borrow_mut()
                .cache_table_detail(qualified_name, *table);
            vec![]
        }

        Effect::EvictTablesFromCompletionCache { tables } => {
            completion_engine.borrow_mut().evict_tables(&tables);
            vec![]
        }

        Effect::ClearCompletionEngineCache => {
            completion_engine.borrow_mut().clear_table_cache();
            vec![]
        }

        Effect::ResizeCompletionCache { capacity } => {
            completion_engine.borrow_mut().resize_cache(capacity);
            vec![]
        }

        Effect::TriggerCompletion => {
            let cursor = state.sql_modal.editor().cursor();
            let content = state.sql_modal.editor().content();
            let database_type = state.session.active_database_type_or_default();
            let active_database = state.session.active_database();

            let (prep, missing) = {
                let engine = completion_engine.borrow();
                let prep = engine.prepare_for_database(content, cursor, database_type);
                let missing = engine.missing_tables_prepared(&prep, state.session.metadata());
                (prep, missing)
            };

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

            let (candidates, token_len, visible) = {
                let engine = completion_engine.borrow();
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
                (candidates, token_len, visible)
            };

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

        let actions = run(
            Effect::TriggerCompletion,
            &state,
            &RefCell::new(CompletionEngine::new()),
        );

        assert!(matches!(
            actions.as_slice(),
            [Action::StartCompletionPrefetch { tables }, Action::CompletionUpdated { .. }]
                if *tables == vec!["public.users".to_string()]
        ));
    }
}
