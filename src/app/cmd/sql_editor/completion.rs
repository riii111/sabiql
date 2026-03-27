use std::cell::RefCell;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::app::cmd::completion_engine::CompletionEngine;
use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::update::action::Action;

pub async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    state: &AppState,
    completion_engine: &RefCell<CompletionEngine>,
) -> Result<()> {
    match effect {
        Effect::CacheTableInCompletionEngine {
            qualified_name,
            table,
        } => {
            completion_engine
                .borrow_mut()
                .cache_table_detail(qualified_name, *table);
            Ok(())
        }

        Effect::EvictTablesFromCompletionCache { tables } => {
            completion_engine.borrow_mut().evict_tables(&tables);
            Ok(())
        }

        Effect::ClearCompletionEngineCache => {
            completion_engine.borrow_mut().clear_table_cache();
            Ok(())
        }

        Effect::ResizeCompletionCache { capacity } => {
            completion_engine.borrow_mut().resize_cache(capacity);
            Ok(())
        }

        Effect::TriggerCompletion => {
            let cursor = state.sql_modal.editor.cursor();
            let content = state.sql_modal.editor.content();

            let (prep, missing) = {
                let engine = completion_engine.borrow();
                let prep = engine.prepare(content, cursor);
                let missing = engine
                    .missing_tables_prepared(&prep, state.session.metadata().map(AsRef::as_ref));
                (prep, missing)
            };

            let prefetch_actions: Vec<Action> = missing
                .into_iter()
                .filter_map(|qualified_name| {
                    qualified_name.split_once('.').map(|(schema, table)| {
                        Action::PrefetchTableDetail {
                            schema: schema.to_string(),
                            table: table.to_string(),
                        }
                    })
                })
                .collect();

            for action in prefetch_actions {
                action_tx.try_send(action).ok();
            }

            let (candidates, token_len, visible) = {
                let engine = completion_engine.borrow();
                let token_len = CompletionEngine::current_token_len_prepared(&prep);
                let recent_cols = state.sql_modal.completion.recent_columns_vec();
                let candidates = engine.get_candidates_prepared(
                    content,
                    cursor,
                    &prep,
                    state.session.metadata().map(AsRef::as_ref),
                    state.session.table_detail(),
                    &recent_cols,
                );
                let visible = !candidates.is_empty() && !content.trim().is_empty();
                (candidates, token_len, visible)
            };

            action_tx
                .send(Action::CompletionUpdated {
                    candidates,
                    trigger_position: cursor.saturating_sub(token_len),
                    visible,
                })
                .await
                .ok();
            Ok(())
        }

        _ => unreachable!("completion::run called with non-completion effect"),
    }
}
