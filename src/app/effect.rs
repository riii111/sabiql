//! Effect types for side effects returned by the reducer.
//!
//! Effects represent I/O operations that must be executed after state transitions.
//! The reducer is pure and returns a `Vec<Effect>` that the EffectRunner executes.

use std::path::PathBuf;
use std::time::Instant;

use crate::domain::Table;

/// Effects are side effects returned by the reducer.
/// They represent I/O operations that must be executed after state transitions.
#[derive(Debug, Clone)]
pub enum Effect {
    // === Rendering ===
    /// Trigger a terminal render
    Render,

    // === Cache Operations ===
    /// Invalidate metadata cache for the given DSN
    CacheInvalidate { dsn: String },
    /// Run periodic cache cleanup
    CacheCleanup,

    // === Metadata Fetching ===
    /// Fetch database metadata from provider
    FetchMetadata { dsn: String },

    // === Table Detail Fetching ===
    /// Fetch table detail for display (updates state.table_detail)
    FetchTableDetail {
        dsn: String,
        schema: String,
        table: String,
        generation: u64,
    },
    /// Prefetch table detail for completion cache (does NOT update state.table_detail)
    PrefetchTableDetail {
        dsn: String,
        schema: String,
        table: String,
    },
    /// Process items from prefetch queue (up to MAX_CONCURRENT_PREFETCH)
    ProcessPrefetchQueue,

    // === Query Execution ===
    /// Execute preview query (SELECT * FROM table LIMIT N)
    ExecutePreview {
        dsn: String,
        schema: String,
        table: String,
        generation: u64,
        limit: usize,
    },
    /// Execute ad-hoc SQL query from SQL modal
    ExecuteAdhoc { dsn: String, query: String },

    // === Completion Engine ===
    /// Cache table detail in completion engine
    CacheTableInCompletionEngine {
        qualified_name: String,
        table: Box<Table>,
    },
    /// Clear completion engine's table cache
    ClearCompletionEngineCache,

    // === Console (EXCLUSIVE - must not run in parallel) ===
    /// Open external console (pgcli/mycli)
    /// This effect requires TUI suspension and exclusive execution
    OpenConsole { dsn: String, project_name: String },

    // === ER Diagram ===
    /// Generate and open ER diagram from cached table data
    GenerateErDiagramFromCache {
        total_tables: usize,
        project_name: String,
    },
    /// Write ER failure log
    WriteErFailureLog {
        failed_tables: Vec<(String, String)>,
        cache_dir: PathBuf,
    },

    // === Debounce ===
    /// Schedule completion trigger after debounce period
    ScheduleCompletionDebounce { trigger_at: Instant },

    // === Ordered Execution ===
    /// Execute effects in sequence (first must complete before second starts)
    /// Used for: CacheInvalidate -> FetchMetadata
    Sequence(Vec<Effect>),
}

impl Effect {
    /// Returns true if this effect requires exclusive TUI access.
    /// Exclusive effects must not run in parallel with other effects.
    pub fn is_exclusive(&self) -> bool {
        matches!(self, Effect::OpenConsole { .. })
    }

    /// Returns true if this is a render effect.
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
        let effect = Effect::Render;
        assert!(!effect.is_exclusive());
    }

    #[test]
    fn fetch_metadata_is_not_exclusive() {
        let effect = Effect::FetchMetadata {
            dsn: "postgres://localhost/test".to_string(),
        };
        assert!(!effect.is_exclusive());
    }

    #[test]
    fn render_is_render() {
        let effect = Effect::Render;
        assert!(effect.is_render());
    }

    #[test]
    fn fetch_metadata_is_not_render() {
        let effect = Effect::FetchMetadata {
            dsn: "postgres://localhost/test".to_string(),
        };
        assert!(!effect.is_render());
    }
}
