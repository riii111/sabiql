//! Executes side effects returned by the reducer.
//!
//! # RefCell Borrow Safety
//!
//! When effects need data from `completion_engine` (a `RefCell`), the borrow
//! MUST be dropped before any await point:
//!
//! ```ignore
//! let tables = {
//!     let engine = completion_engine.borrow();
//!     engine.table_details_iter().map(|...| ...).collect()
//! };  // borrow dropped here
//! some_async_operation(tables).await;  // safe
//! ```

use std::cell::RefCell;
use std::sync::Arc;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::app::action::Action;
use crate::app::completion::CompletionEngine;
use crate::app::effect::Effect;
use crate::app::ports::{ErDiagramExporter, MetadataProvider, QueryExecutor};
use crate::app::state::AppState;
use crate::domain::DatabaseMetadata;
use crate::infra::cache::TtlCache;
use crate::ui::tui::TuiRunner;

pub struct EffectRunner {
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    er_exporter: Arc<dyn ErDiagramExporter>,
    metadata_cache: TtlCache<String, DatabaseMetadata>,
    action_tx: mpsc::Sender<Action>,
}

impl EffectRunner {
    pub fn new(
        metadata_provider: Arc<dyn MetadataProvider>,
        query_executor: Arc<dyn QueryExecutor>,
        er_exporter: Arc<dyn ErDiagramExporter>,
        metadata_cache: TtlCache<String, DatabaseMetadata>,
        action_tx: mpsc::Sender<Action>,
    ) -> Self {
        Self {
            metadata_provider,
            query_executor,
            er_exporter,
            metadata_cache,
            action_tx,
        }
    }

    #[allow(unused_variables)]
    pub async fn run(
        &self,
        effects: Vec<Effect>,
        tui: &mut TuiRunner,
        state: &AppState,
        completion_engine: &RefCell<CompletionEngine>,
    ) -> Result<()> {
        for effect in effects {
            match effect {
                Effect::Sequence(seq_effects) => {
                    for seq_effect in seq_effects {
                        self.run_single(seq_effect, tui, state, completion_engine)
                            .await?;
                    }
                }
                single_effect => {
                    self.run_single(single_effect, tui, state, completion_engine)
                        .await?;
                }
            }
        }
        Ok(())
    }

    #[allow(unused_variables)]
    async fn run_single(
        &self,
        effect: Effect,
        tui: &mut TuiRunner,
        state: &AppState,
        completion_engine: &RefCell<CompletionEngine>,
    ) -> Result<()> {
        if effect.is_exclusive() {
            self.run_exclusive(effect, tui).await
        } else {
            self.run_normal(effect, tui, state, completion_engine).await
        }
    }

    #[allow(unused_variables)]
    async fn run_exclusive(&self, effect: Effect, tui: &mut TuiRunner) -> Result<()> {
        match effect {
            Effect::OpenConsole { dsn, project_name } => {
                // TODO: Phase 4 implementation
                Ok(())
            }
            _ => {
                debug_assert!(
                    false,
                    "Non-exclusive effect passed to run_exclusive: {:?}",
                    effect
                );
                Ok(())
            }
        }
    }

    #[allow(unused_variables, clippy::match_single_binding)]
    async fn run_normal(
        &self,
        effect: Effect,
        tui: &mut TuiRunner,
        state: &AppState,
        completion_engine: &RefCell<CompletionEngine>,
    ) -> Result<()> {
        match effect {
            Effect::Render => Ok(()),
            Effect::ScheduleCompletionDebounce { .. } => Ok(()), // Handled in main loop
            _ => Ok(()),
        }
    }
}
