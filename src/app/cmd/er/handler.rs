use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use super::task::spawn_er_diagram_task;
use crate::cmd::completion_engine::CompletionEngine;
use crate::cmd::effect::Effect;
use crate::domain::er::{er_output_filename, fk_neighbors_of_seeds, fk_reachable_tables_multi};
use crate::domain::{DatabaseMetadata, ErTableInfo, TableSignatureSnapshot};
use crate::model::app_state::AppState;
use crate::ports::outbound::{ConfigWriter, ErDiagramExporter, ErLogWriter, MetadataProvider};
use crate::update::action::{
    Action, SmartErRefreshError, SmartErRefreshFetched, SmartErRefreshResult,
};

struct GenerateErDiagramRequest {
    run_id: u64,
    total_tables: usize,
    project_name: String,
    target_tables: Vec<String>,
    browser: Option<String>,
}

pub async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    er_exporter: &Arc<dyn ErDiagramExporter>,
    config_writer: &Arc<dyn ConfigWriter>,
    er_log_writer: &Arc<dyn ErLogWriter>,
    state: &AppState,
    completion_engine: &RefCell<CompletionEngine>,
) -> Result<()> {
    match effect {
        Effect::GenerateErDiagramFromCache {
            run_id,
            total_tables,
            project_name,
            target_tables,
        } => {
            handle_generate_diagram(
                action_tx,
                er_exporter,
                config_writer,
                completion_engine,
                GenerateErDiagramRequest {
                    run_id,
                    total_tables,
                    project_name,
                    target_tables,
                    browser: state.settings.saved_er_browser().map(str::to_string),
                },
            )
            .await
        }
        Effect::ExtractFkNeighbors {
            run_id,
            seed_tables,
        } => handle_extract_fk_neighbors(action_tx, completion_engine, run_id, seed_tables).await,
        Effect::WriteErFailureLog { failed_tables } => {
            handle_write_failure_log(
                action_tx,
                config_writer,
                er_log_writer,
                state,
                failed_tables,
            )
            .await
        }
        Effect::SmartErRefresh { dsn, run_id } => {
            handle_smart_refresh(action_tx, metadata_provider, dsn, run_id);
            Ok(())
        }
        Effect::SmartErRefreshCacheAndDiff {
            dsn,
            run_id,
            new_metadata,
            signature_snapshot,
        } => {
            handle_smart_refresh_cache_and_diff(
                action_tx,
                state,
                completion_engine,
                dsn,
                run_id,
                new_metadata,
                signature_snapshot,
            )
            .await
        }

        _ => unreachable!("er::run called with non-er effect"),
    }
}

async fn handle_generate_diagram(
    action_tx: &mpsc::Sender<Action>,
    er_exporter: &Arc<dyn ErDiagramExporter>,
    config_writer: &Arc<dyn ConfigWriter>,
    completion_engine: &RefCell<CompletionEngine>,
    request: GenerateErDiagramRequest,
) -> Result<()> {
    let GenerateErDiagramRequest {
        run_id,
        total_tables,
        project_name,
        target_tables,
        browser,
    } = request;
    let all_tables = collect_cached_er_tables(completion_engine);
    if all_tables.is_empty() {
        action_tx
            .send(Action::ErDiagramFailed {
                run_id,
                error: "No table data loaded yet".to_string(),
            })
            .await
            .ok();
        return Ok(());
    }

    let total = all_tables.len();
    let filename = er_output_filename(&target_tables, total);
    let tables = if target_tables.is_empty() || target_tables.len() == total {
        all_tables
    } else {
        fk_reachable_tables_multi(&all_tables, &target_tables, 1)
    };

    if tables.is_empty() {
        action_tx
            .send(Action::ErDiagramFailed {
                run_id,
                error: "Selected tables not found in cached data".to_string(),
            })
            .await
            .ok();
        return Ok(());
    }

    let cache_dir = config_writer.get_cache_dir(&project_name)?;
    spawn_er_diagram_task(
        Arc::clone(er_exporter),
        tables,
        run_id,
        total_tables,
        cache_dir,
        action_tx.clone(),
        filename,
        browser,
    );
    Ok(())
}

async fn handle_extract_fk_neighbors(
    action_tx: &mpsc::Sender<Action>,
    completion_engine: &RefCell<CompletionEngine>,
    run_id: u64,
    seed_tables: Vec<String>,
) -> Result<()> {
    let seed_set: HashSet<&str> = seed_tables.iter().map(String::as_str).collect();
    let (cached_seeds, cached_names) = collect_seed_and_cached_names(completion_engine, &seed_set);
    let neighbors = fk_neighbors_of_seeds(&cached_seeds, &seed_set, &cached_names);

    action_tx
        .send(Action::FkNeighborsDiscovered {
            run_id,
            tables: neighbors,
        })
        .await
        .ok();
    Ok(())
}

async fn handle_write_failure_log(
    action_tx: &mpsc::Sender<Action>,
    config_writer: &Arc<dyn ConfigWriter>,
    er_log_writer: &Arc<dyn ErLogWriter>,
    state: &AppState,
    failed_tables: Vec<(String, String)>,
) -> Result<()> {
    match config_writer.get_cache_dir(state.runtime.project_name()) {
        Ok(cache_dir) => {
            let writer = Arc::clone(er_log_writer);
            let tx = action_tx.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(e) = writer.write_er_failure_log(failed_tables, cache_dir) {
                    tx.blocking_send(Action::ErLogWriteFailed(e.to_string()))
                        .ok();
                }
            });
        }
        Err(e) => {
            action_tx
                .send(Action::ErLogWriteFailed(e.to_string()))
                .await
                .ok();
        }
    }
    Ok(())
}

fn handle_smart_refresh(
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    dsn: String,
    run_id: u64,
) {
    let provider = Arc::clone(metadata_provider);
    let tx = action_tx.clone();

    tokio::spawn(async move {
        let new_metadata = match provider.fetch_metadata(&dsn).await {
            Ok(m) => m,
            Err(e) => {
                tx.send(Action::SmartErRefreshFailed(SmartErRefreshError {
                    dsn,
                    run_id,
                    error: e,
                    new_metadata: None,
                }))
                .await
                .ok();
                return;
            }
        };

        let signature_snapshot = match provider.fetch_table_signatures(&dsn).await {
            Ok(s) => s,
            Err(e) => {
                let new_metadata = Arc::new(new_metadata);
                tx.send(Action::SmartErRefreshFailed(SmartErRefreshError {
                    dsn,
                    run_id,
                    error: e,
                    new_metadata: Some(Arc::clone(&new_metadata)),
                }))
                .await
                .ok();
                return;
            }
        };

        tx.send(Action::SmartErRefreshFetched(SmartErRefreshFetched {
            dsn,
            run_id,
            new_metadata: Arc::new(new_metadata),
            signature_snapshot: Arc::new(signature_snapshot),
        }))
        .await
        .ok();
    });
}

async fn handle_smart_refresh_cache_and_diff(
    action_tx: &mpsc::Sender<Action>,
    state: &AppState,
    completion_engine: &RefCell<CompletionEngine>,
    dsn: String,
    run_id: u64,
    new_metadata: Arc<DatabaseMetadata>,
    signature_snapshot: Arc<TableSignatureSnapshot>,
) -> Result<()> {
    if !state.session.dsn_matches(&dsn) || !state.er_preparation.is_current_run(run_id) {
        return Ok(());
    }

    let TableSignatureSnapshot {
        signatures,
        prefetched_table_details,
    } = Arc::unwrap_or_clone(signature_snapshot);
    {
        let mut engine = completion_engine.borrow_mut();
        for detail in prefetched_table_details {
            engine.cache_table_detail(detail.qualified_name(), detail);
        }
    }

    let old_signatures = state.er_preparation.last_signatures().clone();
    let cached_tables = collect_cached_table_names(completion_engine);
    let new_signatures: std::collections::HashMap<String, String> = signatures
        .iter()
        .map(|signature| (signature.qualified_name(), signature.signature.clone()))
        .collect();

    let old_names: HashSet<&str> = old_signatures.keys().map(String::as_str).collect();
    let new_names: HashSet<&str> = new_signatures.keys().map(String::as_str).collect();

    let added_tables: Vec<String> = new_names
        .difference(&old_names)
        .filter(|name| !cached_tables.contains(**name))
        .map(ToString::to_string)
        .collect();
    let removed_tables: Vec<String> = old_names
        .difference(&new_names)
        .map(ToString::to_string)
        .collect();

    let stale_tables: Vec<String> = new_signatures
        .iter()
        .filter(|(name, sig)| {
            old_signatures
                .get(name.as_str())
                .is_some_and(|old_sig| old_sig != *sig)
        })
        .map(|(name, _)| name.clone())
        .collect();

    let missing_in_cache: Vec<String> = new_names
        .iter()
        .filter(|name| !cached_tables.contains(**name))
        .map(ToString::to_string)
        .collect();

    action_tx
        .send(Action::SmartErRefreshCompleted(SmartErRefreshResult {
            dsn,
            run_id,
            new_metadata,
            stale_tables,
            added_tables,
            removed_tables,
            missing_in_cache,
            new_signatures,
        }))
        .await
        .ok();
    Ok(())
}

fn collect_cached_er_tables(completion_engine: &RefCell<CompletionEngine>) -> Vec<ErTableInfo> {
    let engine = completion_engine.borrow();
    engine
        .table_details_iter()
        .map(|(name, table)| ErTableInfo::from_table(name, table))
        .collect()
}

fn collect_seed_and_cached_names(
    completion_engine: &RefCell<CompletionEngine>,
    seed_set: &HashSet<&str>,
) -> (Vec<ErTableInfo>, HashSet<String>) {
    let engine = completion_engine.borrow();
    let seeds = engine
        .table_details_iter()
        .filter(|(name, _)| seed_set.contains(name.as_str()))
        .map(|(name, table)| ErTableInfo::from_table(name, table))
        .collect();
    let all_cached = engine
        .table_details_iter()
        .map(|(name, _)| name.clone())
        .collect();
    (seeds, all_cached)
}

fn collect_cached_table_names(completion_engine: &RefCell<CompletionEngine>) -> HashSet<String> {
    let engine = completion_engine.borrow();
    engine
        .table_details_iter()
        .map(|(name, _)| name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Column, ColumnAttributes, ConnectionId, DatabaseType, Table, TableKindInfo, TableSignature,
        TableStorageAttributes,
    };

    fn state_with_mysql_dsn(dsn: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "mysql",
            DatabaseType::MySQL,
            dsn,
        );
        let _ = state.er_preparation.start_waiting_run();
        state
    }

    fn light_table_detail() -> Table {
        Table {
            schema: "app".to_string(),
            name: "items".to_string(),
            owner: None,
            columns: vec![Column {
                name: "id".to_string(),
                data_type: "int".to_string(),
                default: None,
                attributes: ColumnAttributes::from_parts(false, true, false),
                comment: None,
                ordinal_position: 1,
                character_set_name: None,
                collation_name: None,
                generation_expression: None,
                generation_kind: None,
            }],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
            rls: None,
            triggers: Vec::new(),
            row_count_estimate: None,
            comment: None,
            source_ddl: None,
            storage_attributes: TableStorageAttributes::default(),
            kind_info: TableKindInfo::default(),
        }
    }

    #[tokio::test]
    async fn signature_details_seed_empty_completion_cache_before_diff() {
        let dsn = "mysql://user:password@localhost:3306/app";
        let state = state_with_mysql_dsn(dsn);
        let completion_engine = RefCell::new(CompletionEngine::new());
        let (action_tx, mut action_rx) = mpsc::channel(1);
        let table = light_table_detail();

        handle_smart_refresh_cache_and_diff(
            &action_tx,
            &state,
            &completion_engine,
            dsn.to_string(),
            1,
            Arc::new(DatabaseMetadata::new("app".to_string())),
            Arc::new(TableSignatureSnapshot {
                signatures: vec![TableSignature {
                    schema: "app".to_string(),
                    name: "items".to_string(),
                    signature: "signature".to_string(),
                }],
                prefetched_table_details: vec![table],
            }),
        )
        .await
        .unwrap();

        assert!(completion_engine.borrow().has_cached_table("app.items"));
        let Action::SmartErRefreshCompleted(result) = action_rx.recv().await.unwrap() else {
            panic!("expected smart refresh completion");
        };
        assert!(result.added_tables.is_empty());
        assert!(result.missing_in_cache.is_empty());
    }
}
