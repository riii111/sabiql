use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

use color_eyre::eyre::Result;

use super::EffectContext;
use crate::app::action::{Action, SmartErRefreshError, SmartErRefreshResult};
use crate::app::completion::CompletionEngine;
use crate::app::effect::Effect;
use crate::app::er_task::spawn_er_diagram_task;
use crate::app::state::AppState;
use crate::domain::ErTableInfo;

pub(crate) async fn run(
    effect: Effect,
    ctx: &EffectContext<'_>,
    state: &mut AppState,
    completion_engine: &RefCell<CompletionEngine>,
) -> Result<()> {
    match effect {
        Effect::GenerateErDiagramFromCache {
            total_tables,
            project_name,
            target_tables,
        } => {
            use crate::domain::er::{er_output_filename, fk_reachable_tables_multi};

            let all_tables: Vec<ErTableInfo> = {
                let engine = completion_engine.borrow();
                engine
                    .table_details_iter()
                    .map(|(k, v)| ErTableInfo::from_table(k, v))
                    .collect()
            };

            if all_tables.is_empty() {
                ctx.action_tx
                    .send(Action::ErDiagramFailed(
                        "No table data loaded yet".to_string(),
                    ))
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
                ctx.action_tx
                    .send(Action::ErDiagramFailed(
                        "Selected tables not found in cached data".to_string(),
                    ))
                    .await
                    .ok();
                return Ok(());
            }

            let cache_dir = ctx.config_writer.get_cache_dir(&project_name)?;
            let exporter = Arc::clone(ctx.er_exporter);
            spawn_er_diagram_task(
                exporter,
                tables,
                total_tables,
                cache_dir,
                ctx.action_tx.clone(),
                filename,
            );
            Ok(())
        }

        Effect::ExtractFkNeighbors { seed_tables } => {
            use crate::domain::er::fk_neighbors_of_seeds;

            let seed_set: HashSet<&str> = seed_tables.iter().map(|s| s.as_str()).collect();

            let (cached_seeds, cached_names): (Vec<ErTableInfo>, HashSet<String>) = {
                let engine = completion_engine.borrow();
                let seeds: Vec<ErTableInfo> = engine
                    .table_details_iter()
                    .filter(|(k, _)| seed_set.contains(k.as_str()))
                    .map(|(k, v)| ErTableInfo::from_table(k, v))
                    .collect();
                let all_cached: HashSet<String> = engine
                    .table_details_iter()
                    .map(|(k, _)| k.clone())
                    .collect();
                (seeds, all_cached)
            };

            let neighbors = fk_neighbors_of_seeds(&cached_seeds, &seed_set, &cached_names);

            ctx.action_tx
                .send(Action::FkNeighborsDiscovered { tables: neighbors })
                .await
                .ok();
            Ok(())
        }

        Effect::WriteErFailureLog { failed_tables } => {
            match ctx.config_writer.get_cache_dir(&state.runtime.project_name) {
                Ok(cache_dir) => {
                    let writer = Arc::clone(ctx.er_log_writer);
                    let tx = ctx.action_tx.clone();
                    tokio::task::spawn_blocking(move || {
                        if let Err(e) = writer.write_er_failure_log(failed_tables, cache_dir) {
                            tx.blocking_send(Action::ErLogWriteFailed(e.to_string()))
                                .ok();
                        }
                    });
                }
                Err(e) => {
                    ctx.action_tx
                        .send(Action::ErLogWriteFailed(e.to_string()))
                        .await
                        .ok();
                }
            }
            Ok(())
        }

        Effect::SmartErRefresh { dsn, run_id } => {
            let provider = Arc::clone(ctx.metadata_provider);
            let tx = ctx.action_tx.clone();

            let old_signatures = state.er_preparation.last_signatures.clone();
            let cached_tables: HashSet<String> = {
                let engine = completion_engine.borrow();
                engine
                    .table_details_iter()
                    .map(|(k, _)| k.clone())
                    .collect()
            };

            tokio::spawn(async move {
                let new_metadata = match provider.fetch_metadata(&dsn).await {
                    Ok(m) => m,
                    Err(e) => {
                        tx.send(Action::SmartErRefreshFailed(SmartErRefreshError {
                            run_id,
                            error: e.to_string(),
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
                        tx.send(Action::SmartErRefreshFailed(SmartErRefreshError {
                            run_id,
                            error: e.to_string(),
                            new_metadata: Some(Box::new(new_metadata)),
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

                let old_names: HashSet<&str> = old_signatures.keys().map(|s| s.as_str()).collect();
                let new_names: HashSet<&str> = new_signatures.keys().map(|s| s.as_str()).collect();

                let added_tables: Vec<String> = new_names
                    .difference(&old_names)
                    .map(|s| s.to_string())
                    .collect();
                let removed_tables: Vec<String> = old_names
                    .difference(&new_names)
                    .map(|s| s.to_string())
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
                    .map(|s| s.to_string())
                    .collect();

                tx.send(Action::SmartErRefreshCompleted(SmartErRefreshResult {
                    run_id,
                    new_metadata: Box::new(new_metadata),
                    stale_tables,
                    added_tables,
                    removed_tables,
                    missing_in_cache,
                    new_signatures,
                }))
                .await
                .ok();
            });
            Ok(())
        }

        _ => unreachable!("er::run called with non-er effect"),
    }
}
