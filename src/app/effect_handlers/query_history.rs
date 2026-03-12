use std::sync::Arc;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::ports::QueryHistoryStore;

pub(crate) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    query_history_store: &Arc<dyn QueryHistoryStore>,
) -> Result<()> {
    match effect {
        Effect::LoadQueryHistory {
            project_name,
            connection_id,
        } => {
            let store = Arc::clone(query_history_store);
            let tx = action_tx.clone();

            tokio::spawn(async move {
                match store.load(&project_name, &connection_id).await {
                    Ok(entries) => {
                        tx.send(Action::QueryHistoryLoaded(entries)).await.ok();
                    }
                    Err(e) => {
                        eprintln!("Failed to load query history: {}", e);
                        tx.send(Action::QueryHistoryLoaded(Vec::new())).await.ok();
                    }
                }
            });
            Ok(())
        }
        _ => unreachable!("query_history::run called with non-query-history effect"),
    }
}
