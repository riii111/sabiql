use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use super::task::spawn_er_diagram_task;
use crate::app::cmd::completion_engine::CompletionEngine;
use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::ports::outbound::{ConfigWriter, ErDiagramExporter, ErLogWriter, MetadataProvider};
use crate::app::update::action::{
    Action, ErDiagramError, ErLogError, SmartErRefreshError, SmartErRefreshResult,
};
use crate::domain::ErTableInfo;
use crate::domain::er::{er_output_filename, fk_neighbors_of_seeds, fk_reachable_tables_multi};

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
            total_tables,
            project_name,
            target_tables,
        } => {
            handle_generate_diagram(
                action_tx,
                er_exporter,
                config_writer,
                completion_engine,
                total_tables,
                project_name,
                target_tables,
            )
            .await
        }
        Effect::ExtractFkNeighbors { seed_tables } => {
            handle_extract_fk_neighbors(action_tx, completion_engine, seed_tables).await
        }
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
            handle_smart_refresh(
                action_tx,
                metadata_provider,
                state,
                completion_engine,
                dsn,
                run_id,
            );
            Ok(())
        }

        _ => unreachable!("er::run called with non-er effect"),
    }
}

async fn handle_generate_diagram(
    action_tx: &mpsc::Sender<Action>,
    er_exporter: &Arc<dyn ErDiagramExporter>,
    config_writer: &Arc<dyn ConfigWriter>,
    completion_engine: &RefCell<CompletionEngine>,
    total_tables: usize,
    project_name: String,
    target_tables: Vec<String>,
) -> Result<()> {
    let all_tables = collect_cached_er_tables(completion_engine);
    if all_tables.is_empty() {
        action_tx
            .send(Action::ErDiagramFailed(ErDiagramError::NoData(
                "No table data loaded yet".to_string(),
            )))
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
            .send(Action::ErDiagramFailed(ErDiagramError::NoData(
                "Selected tables not found in cached data".to_string(),
            )))
            .await
            .ok();
        return Ok(());
    }

    let cache_dir = config_writer.get_cache_dir(&project_name)?;
    spawn_er_diagram_task(
        Arc::clone(er_exporter),
        tables,
        total_tables,
        cache_dir,
        action_tx.clone(),
        filename,
    );
    Ok(())
}

async fn handle_extract_fk_neighbors(
    action_tx: &mpsc::Sender<Action>,
    completion_engine: &RefCell<CompletionEngine>,
    seed_tables: Vec<String>,
) -> Result<()> {
    let seed_set: HashSet<&str> = seed_tables.iter().map(String::as_str).collect();
    let (cached_seeds, cached_names) = collect_seed_and_cached_names(completion_engine, &seed_set);
    let neighbors = fk_neighbors_of_seeds(&cached_seeds, &seed_set, &cached_names);

    action_tx
        .send(Action::FkNeighborsDiscovered { tables: neighbors })
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
    match config_writer.get_cache_dir(&state.runtime.project_name) {
        Ok(cache_dir) => {
            let writer = Arc::clone(er_log_writer);
            let tx = action_tx.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(e) = writer.write_er_failure_log(failed_tables, cache_dir) {
                    tx.blocking_send(Action::ErLogWriteFailed(ErLogError::Io(e.to_string())))
                        .ok();
                }
            });
        }
        Err(e) => {
            action_tx
                .send(Action::ErLogWriteFailed(ErLogError::Config(e.to_string())))
                .await
                .ok();
        }
    }
    Ok(())
}

fn handle_smart_refresh(
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    state: &AppState,
    completion_engine: &RefCell<CompletionEngine>,
    dsn: String,
    run_id: u64,
) {
    let provider = Arc::clone(metadata_provider);
    let tx = action_tx.clone();
    let old_signatures = state.er_preparation.last_signatures.clone();
    let cached_tables = collect_cached_table_names(completion_engine);

    tokio::spawn(async move {
        let new_metadata = match provider.fetch_metadata(&dsn).await {
            Ok(m) => m,
            Err(e) => {
                tx.send(Action::SmartErRefreshFailed(SmartErRefreshError {
                    run_id,
                    error: e,
                    new_metadata: None,
                }))
                .await
                .ok();
                return;
            }
        };

        let new_sigs_vec = match provider.fetch_table_signatures(&dsn).await {
            Ok(s) => s,
            Err(e) => {
                let new_metadata = Arc::new(new_metadata);
                tx.send(Action::SmartErRefreshFailed(SmartErRefreshError {
                    run_id,
                    error: e,
                    new_metadata: Some(Arc::clone(&new_metadata)),
                }))
                .await
                .ok();
                return;
            }
        };

        let new_signatures: std::collections::HashMap<String, String> = new_sigs_vec
            .iter()
            .map(|s| (s.qualified_name(), s.signature.clone()))
            .collect();

        let old_names: HashSet<&str> = old_signatures.keys().map(String::as_str).collect();
        let new_names: HashSet<&str> = new_signatures.keys().map(String::as_str).collect();

        let added_tables: Vec<String> = new_names
            .difference(&old_names)
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

        tx.send(Action::SmartErRefreshCompleted(SmartErRefreshResult {
            run_id,
            new_metadata: Arc::new(new_metadata),
            stale_tables,
            added_tables,
            removed_tables,
            missing_in_cache,
            new_signatures,
        }))
        .await
        .ok();
    });
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
