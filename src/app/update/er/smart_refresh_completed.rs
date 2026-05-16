use std::sync::Arc;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::{Action, SmartErRefreshResult};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_smart_refresh_completed(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        Action::SmartErRefreshCompleted(SmartErRefreshResult {
            run_id,
            new_metadata,
            stale_tables,
            added_tables,
            removed_tables,
            missing_in_cache,
            new_signatures,
        }) => {
            if *run_id != state.er_preparation.run_id {
                return DispatchResult::handled();
            }

            state.session.set_metadata(Some(Arc::clone(new_metadata)));
            state
                .er_preparation
                .last_signatures
                .clone_from(new_signatures);
            state.er_preparation.total_tables = new_metadata.table_summaries.len();

            let mut effects: Vec<Effect> = Vec::new();

            if !removed_tables.is_empty() {
                effects.push(Effect::EvictTablesFromCompletionCache {
                    tables: removed_tables.clone(),
                });
            }

            let mut refetch: Vec<String> = stale_tables
                .iter()
                .chain(added_tables)
                .chain(missing_in_cache)
                .cloned()
                .collect();
            refetch.sort();
            refetch.dedup();

            if refetch.is_empty() {
                state.set_success(
                    "No schema changes detected, generating ER diagram...".to_string(),
                );
                effects.push(Effect::DispatchActions(vec![Action::ErGenerateFromCache]));
            } else {
                if !stale_tables.is_empty() {
                    effects.push(Effect::EvictTablesFromCompletionCache {
                        tables: stale_tables.clone(),
                    });
                }
                state.set_success(format!(
                    "Refreshing {} table(s) for ER diagram...",
                    refetch.len()
                ));
                effects.push(Effect::DispatchActions(vec![Action::StartPrefetchScoped {
                    tables: refetch,
                }]));
            }

            DispatchResult::handled_with(effects)
        }
        _ => DispatchResult::pass(),
    }
}
