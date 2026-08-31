use std::sync::Arc;

use tokio::sync::mpsc;

use crate::cmd::effect::Effect;
use crate::ports::outbound::QueryHistoryStore;
use crate::update::action::Action;

pub fn spawn_query_history_load(
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

            tokio::spawn(async move {
                let action = match store.load(&project_name, &scope).await {
                    Ok(entries) => Action::QueryHistoryLoaded(scope, entries),
                    Err(e) => Action::QueryHistoryLoadFailed(scope, e.to_string()),
                };
                tx.send(action).await.ok();
            });
        }
        _ => unreachable!("spawn_query_history_load called with non-query-history effect"),
    }
}
