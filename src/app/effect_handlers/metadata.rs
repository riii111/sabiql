use std::cell::RefCell;
use std::sync::Arc;

use color_eyre::eyre::Result;

use super::EffectContext;
use crate::app::action::Action;
use crate::app::completion::CompletionEngine;
use crate::app::effect::Effect;
use crate::app::state::AppState;

pub(crate) async fn run(
    effect: Effect,
    ctx: &EffectContext<'_>,
    _state: &mut AppState,
    completion_engine: &RefCell<CompletionEngine>,
) -> Result<()> {
    match effect {
        Effect::FetchMetadata { dsn } => {
            if let Some(cached) = ctx.metadata_cache.get(&dsn).await {
                ctx.action_tx
                    .send(Action::MetadataLoaded(Box::new(cached)))
                    .await
                    .ok();
            } else {
                let provider = Arc::clone(ctx.metadata_provider);
                let cache = ctx.metadata_cache.clone();
                let tx = ctx.action_tx.clone();

                tokio::spawn(async move {
                    match provider.fetch_metadata(&dsn).await {
                        Ok(metadata) => {
                            cache.set(dsn, metadata.clone()).await;
                            tx.send(Action::MetadataLoaded(Box::new(metadata)))
                                .await
                                .ok();
                        }
                        Err(e) => {
                            tx.send(Action::MetadataFailed(e.to_string())).await.ok();
                        }
                    }
                });
            }
            Ok(())
        }

        Effect::FetchTableDetail {
            dsn,
            schema,
            table,
            generation,
        } => {
            let provider = Arc::clone(ctx.metadata_provider);
            let tx = ctx.action_tx.clone();

            tokio::spawn(async move {
                match provider.fetch_table_detail(&dsn, &schema, &table).await {
                    Ok(detail) => {
                        tx.send(Action::TableDetailLoaded(Box::new(detail), generation))
                            .await
                            .ok();
                    }
                    Err(e) => {
                        tx.send(Action::TableDetailFailed(e.to_string(), generation))
                            .await
                            .ok();
                    }
                }
            });
            Ok(())
        }

        Effect::PrefetchTableDetail { dsn, schema, table } => {
            let qualified_name = format!("{}.{}", schema, table);

            let already_cached = completion_engine.borrow().has_cached_table(&qualified_name);
            if already_cached {
                ctx.action_tx
                    .send(Action::TableDetailAlreadyCached { schema, table })
                    .await
                    .ok();
                return Ok(());
            }

            let provider = Arc::clone(ctx.metadata_provider);
            let tx = ctx.action_tx.clone();

            tokio::spawn(async move {
                let result = tokio::time::timeout(
                    tokio::time::Duration::from_secs(10),
                    provider.fetch_table_detail_light(&dsn, &schema, &table),
                )
                .await;
                match result {
                    Ok(Ok(detail)) => {
                        tx.send(Action::TableDetailCached {
                            schema,
                            table,
                            detail: Box::new(detail),
                        })
                        .await
                        .ok();
                    }
                    Ok(Err(e)) => {
                        tx.send(Action::TableDetailCacheFailed {
                            schema,
                            table,
                            error: e.to_string(),
                        })
                        .await
                        .ok();
                    }
                    Err(_) => {
                        tx.send(Action::TableDetailCacheFailed {
                            schema,
                            table,
                            error: "prefetch timeout".to_string(),
                        })
                        .await
                        .ok();
                    }
                }
            });
            Ok(())
        }

        Effect::ProcessPrefetchQueue => {
            ctx.action_tx.send(Action::ProcessPrefetchQueue).await.ok();
            Ok(())
        }

        Effect::DelayedProcessPrefetchQueue { delay_secs } => {
            let tx = ctx.action_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
                tx.send(Action::ProcessPrefetchQueue).await.ok();
            });
            Ok(())
        }

        Effect::CacheInvalidate { dsn } => {
            ctx.metadata_cache.invalidate(&dsn).await;
            Ok(())
        }

        _ => unreachable!("metadata::run called with non-metadata effect"),
    }
}
