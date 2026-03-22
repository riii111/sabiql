use std::sync::Arc;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::app::cmd::effect::Effect;
use crate::app::ports::QueryHistoryStore;
use crate::app::update::action::Action;

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

            let conn_id = connection_id.clone();
            tokio::spawn(async move {
                match store.load(&project_name, &connection_id).await {
                    Ok(entries) => {
                        tx.send(Action::QueryHistoryLoaded(conn_id, entries))
                            .await
                            .ok();
                    }
                    Err(e) => {
                        tx.send(Action::QueryHistoryLoadFailed(e)).await.ok();
                    }
                }
            });
            Ok(())
        }
        _ => unreachable!("query_history::run called with non-query-history effect"),
    }
}
