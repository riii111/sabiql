use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::{DatabaseMetadata, TableSignature, TableSignatureSnapshot};
use crate::model::app_state::AppState;
use crate::model::table_detail_cache::TableDetailCache;
use crate::update::action::{Action, SmartErRefreshFetched};
use crate::update::helpers::reject_pending_mysql_connection_probe;

use super::diagram::generate_from_cache;

pub(super) struct RefreshDiff {
    pub(super) stale_tables: Vec<String>,
    pub(super) removed_tables: Vec<String>,
    pub(super) missing_in_cache: Vec<String>,
    pub(super) new_signatures: HashMap<String, String>,
}

pub(in crate::update) fn smart_er_refresh_fetched(
    state: &mut AppState,
    fetched: SmartErRefreshFetched,
    now: Instant,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    let SmartErRefreshFetched {
        endpoint,
        run_id,
        new_metadata,
        signature_snapshot,
    } = fetched;
    if !state.session.endpoint_matches(&endpoint) || !state.er_preparation.is_current_run(run_id) {
        return vec![];
    }

    let TableSignatureSnapshot {
        signatures,
        prefetched_table_details,
    } = Arc::unwrap_or_clone(signature_snapshot);
    for detail in prefetched_table_details {
        state
            .table_detail_cache
            .insert(detail.qualified_name(), detail);
    }
    let diff = diff_signatures(
        state.er_preparation.last_signatures(),
        &signatures,
        &state.table_detail_cache,
    );

    apply_refresh_diff(state, new_metadata, diff, now)
}

pub(super) fn apply_refresh_diff(
    state: &mut AppState,
    new_metadata: Arc<DatabaseMetadata>,
    diff: RefreshDiff,
    now: Instant,
) -> Vec<Effect> {
    let RefreshDiff {
        stale_tables,
        removed_tables,
        missing_in_cache,
        new_signatures,
    } = diff;
    let table_count = new_metadata.table_summaries.len();
    state.session.set_metadata(Some(new_metadata));
    state
        .er_preparation
        .apply_refresh_metadata(new_signatures, table_count);
    state.table_detail_cache.evict(&removed_tables);

    let mut refetch: Vec<String> = stale_tables
        .iter()
        .chain(&missing_in_cache)
        .cloned()
        .collect();
    refetch.sort();
    refetch.dedup();

    if refetch.is_empty() {
        state.messages.set_success_at(
            "No schema changes detected, generating ER diagram...".to_string(),
            now,
        );
        return generate_from_cache(state);
    }

    state.table_detail_cache.evict(&stale_tables);
    state.messages.set_success_at(
        format!("Refreshing {} table(s) for ER diagram...", refetch.len()),
        now,
    );
    vec![Effect::DispatchActions(vec![
        Action::StartErPrefetchScoped { tables: refetch },
    ])]
}

fn diff_signatures(
    old_signatures: &HashMap<String, String>,
    signatures: &[TableSignature],
    table_detail_cache: &TableDetailCache,
) -> RefreshDiff {
    let new_signatures: HashMap<String, String> = signatures
        .iter()
        .map(|signature| (signature.qualified_name(), signature.signature.clone()))
        .collect();

    let old_names: HashSet<&str> = old_signatures.keys().map(String::as_str).collect();
    let new_names: HashSet<&str> = new_signatures.keys().map(String::as_str).collect();

    let removed_tables = old_names
        .difference(&new_names)
        .map(ToString::to_string)
        .collect();
    let stale_tables = new_signatures
        .iter()
        .filter(|(name, signature)| {
            old_signatures
                .get(name.as_str())
                .is_some_and(|old_signature| old_signature != *signature)
        })
        .map(|(name, _)| name.clone())
        .collect();
    let missing_in_cache = new_names
        .iter()
        .filter(|name| !table_detail_cache.contains(name))
        .map(ToString::to_string)
        .collect();

    RefreshDiff {
        stale_tables,
        removed_tables,
        missing_in_cache,
        new_signatures,
    }
}
