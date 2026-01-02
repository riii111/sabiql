//! Side effects returned by the reducer, executed by EffectRunner.

use std::path::PathBuf;
use std::time::Instant;

use crate::app::action::Action;
use crate::domain::Table;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Effect {
    Render,

    CacheInvalidate {
        dsn: String,
    },
    CacheCleanup,

    FetchMetadata {
        dsn: String,
    },

    /// Updates state.table_detail on completion
    FetchTableDetail {
        dsn: String,
        schema: String,
        table: String,
        generation: u64,
    },
    /// Only caches in completion_engine, does NOT update state.table_detail
    PrefetchTableDetail {
        dsn: String,
        schema: String,
        table: String,
    },
    ProcessPrefetchQueue,

    ExecutePreview {
        dsn: String,
        schema: String,
        table: String,
        generation: u64,
        limit: usize,
    },
    ExecuteAdhoc {
        dsn: String,
        query: String,
    },

    CacheTableInCompletionEngine {
        qualified_name: String,
        table: Box<Table>,
    },
    ClearCompletionEngineCache,

    /// Requires TUI suspension - must not run in parallel with other effects
    OpenConsole {
        dsn: String,
        project_name: String,
    },

    GenerateErDiagramFromCache {
        total_tables: usize,
        project_name: String,
    },
    WriteErFailureLog {
        failed_tables: Vec<(String, String)>,
        cache_dir: PathBuf,
    },

    ScheduleCompletionDebounce {
        trigger_at: Instant,
    },

    /// Triggers completion: fetches missing tables and updates candidates
    TriggerCompletion,

    /// Ensures ordering: e.g., CacheInvalidate must complete before FetchMetadata
    Sequence(Vec<Effect>),

    /// Dispatch actions to be processed by the reducer
    DispatchActions(Vec<Action>),
}

#[allow(dead_code)]
impl Effect {
    /// OpenConsole requires TUI suspension and blocks other effects
    pub fn is_exclusive(&self) -> bool {
        matches!(self, Effect::OpenConsole { .. })
    }

    pub fn is_render(&self) -> bool {
        matches!(self, Effect::Render)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_console_is_exclusive() {
        let effect = Effect::OpenConsole {
            dsn: "postgres://localhost/test".to_string(),
            project_name: "test".to_string(),
        };
        assert!(effect.is_exclusive());
    }

    #[test]
    fn render_is_not_exclusive() {
        assert!(!Effect::Render.is_exclusive());
    }

    #[test]
    fn fetch_metadata_is_not_exclusive() {
        let effect = Effect::FetchMetadata {
            dsn: "postgres://localhost/test".to_string(),
        };
        assert!(!effect.is_exclusive());
    }
}
