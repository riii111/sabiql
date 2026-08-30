use std::sync::Arc;

use tokio::sync::mpsc;

use crate::cmd::effect::Effect;
use crate::ports::outbound::QueryHistoryStore;
use crate::update::action::Action;

pub fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    query_history_store: &Arc<dyn QueryHistoryStore>,
) {
    match effect {
        Effect::LoadQueryHistory {
            project_name,
            scope,
        } => {
            let store = Arc::clone(query_history_store);
            let tx = action_tx.clone();

            let scope_for_action = scope.clone();
            tokio::spawn(async move {
                match store.load(&project_name, &scope).await {
                    Ok(entries) => {
                        tx.send(Action::QueryHistoryLoaded(
                            scope_for_action.clone(),
                            entries,
                        ))
                        .await
                        .ok();
                    }
                    Err(e) => {
                        tx.send(Action::QueryHistoryLoadFailed(scope_for_action, e))
                            .await
                            .ok();
                    }
                }
            });
        }
        _ => unreachable!("query_history::run called with non-query-history effect"),
    }
}
