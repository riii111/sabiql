use std::sync::Arc;

use crate::domain::query_history::QueryHistoryScope;
use crate::ports::outbound::QueryHistoryStore;
use crate::update::action::Action;

pub(in crate::cmd) async fn load_query_history(
    store: Arc<dyn QueryHistoryStore>,
    project_name: String,
    scope: QueryHistoryScope,
) -> Action {
    let outcome = store
        .load(&project_name, &scope)
        .await
        .map_err(|error| error.to_string());
    Action::QueryHistoryLoaded(scope, outcome)
}
