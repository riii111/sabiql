//! EffectRunner executes side effects returned by the reducer.
//!
//! The EffectRunner is responsible for:
//! - Executing I/O operations (database queries, file operations)
//! - Spawning async tasks
//! - Managing TUI suspension for exclusive effects (e.g., Console)
//! - Sending result actions back through the action channel
//!
//! # RefCell Borrow Safety
//!
//! When effects need data from `completion_engine` (a `RefCell`), the borrow
//! MUST be dropped before any await point. Extract all needed data in a scoped
//! borrow block, then use the copied data for async operations.
//!
//! ```ignore
//! // CORRECT: borrow is scoped and short-lived
//! let tables = {
//!     let engine = completion_engine.borrow();
//!     engine.table_details_iter().map(|...| ...).collect()
//! };  // borrow dropped here
//!
//! // Now safe to await
//! some_async_operation(tables).await;
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

/// Executes side effects returned by the reducer.
pub struct EffectRunner {
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    er_exporter: Arc<dyn ErDiagramExporter>,
    metadata_cache: TtlCache<String, DatabaseMetadata>,
    action_tx: mpsc::Sender<Action>,
}

impl EffectRunner {
    /// Create a new EffectRunner with all required dependencies.
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

    /// Execute a list of effects.
    ///
    /// Effects are executed in order. Exclusive effects (like OpenConsole)
    /// block all other effects until completion.
    ///
    /// # Arguments
    ///
    /// * `effects` - List of effects to execute
    /// * `tui` - TUI runner (needed for exclusive effects and rendering)
    /// * `state` - Current application state (for rendering)
    /// * `completion_engine` - Completion engine (RefCell - see borrow safety docs)
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
                    // Execute sequentially, waiting for each to complete
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

    /// Execute a single effect.
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

    /// Execute an exclusive effect (requires TUI suspension).
    #[allow(unused_variables)]
    async fn run_exclusive(&self, effect: Effect, tui: &mut TuiRunner) -> Result<()> {
        match effect {
            Effect::OpenConsole { dsn, project_name } => {
                // Phase 4: Implement console execution with TUI suspension
                // 1. Create suspend guard
                // 2. Execute pgcli in blocking task
                // 3. Resume TUI
                // 4. Send Render action
                Ok(())
            }
            _ => {
                // Non-exclusive effects should not reach here
                debug_assert!(
                    false,
                    "Non-exclusive effect passed to run_exclusive: {:?}",
                    effect
                );
                Ok(())
            }
        }
    }

    /// Execute a normal (non-exclusive) effect.
    #[allow(unused_variables, clippy::match_single_binding)]
    async fn run_normal(
        &self,
        effect: Effect,
        tui: &mut TuiRunner,
        state: &AppState,
        completion_engine: &RefCell<CompletionEngine>,
    ) -> Result<()> {
        match effect {
            // Phase 2-4: Effects will be implemented here
            Effect::Render => {
                // Will be implemented: draw to terminal
                Ok(())
            }
            Effect::ScheduleCompletionDebounce { trigger_at } => {
                // Debounce is handled in main loop, not here
                // This effect is a no-op in EffectRunner
                Ok(())
            }
            _ => {
                // Placeholder for unimplemented effects
                Ok(())
            }
        }
    }
}
