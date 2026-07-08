use crate::domain::{DiagnosticField, SqliteDiagnosticsSnapshot};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum LoadState {
    #[default]
    Idle,
    LoadingCore {
        run_id: u64,
        pending_quick_check: Option<DiagnosticField>,
    },
    Loaded {
        run_id: u64,
        snapshot: Box<SqliteDiagnosticsSnapshot>,
        quick_check_running: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticFieldKind {
    DbFile,
    SqliteVersion,
    ForeignKeys,
    JournalMode,
    QueryOnly,
    BusyTimeout,
    DatabaseList,
    QuickCheck,
}

impl DiagnosticFieldKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::DbFile => "Database file",
            Self::SqliteVersion => "SQLite version",
            Self::ForeignKeys => "Effective foreign keys",
            Self::JournalMode => "Journal mode",
            Self::QueryOnly => "Effective query only",
            Self::BusyTimeout => "Effective busy timeout (ms)",
            Self::DatabaseList => "Database list",
            Self::QuickCheck => "Quick check",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticDisplayRow {
    pub kind: DiagnosticFieldKind,
    pub value: String,
}

#[derive(Debug, Clone, Default)]
pub struct SqliteDiagnosticsState {
    next_run_id: u64,
    load_state: LoadState,
    scroll_offset: usize,
    content_line_count: Option<usize>,
    visible_rows: Option<usize>,
}

impl SqliteDiagnosticsState {
    pub fn begin_fetch(&mut self) -> u64 {
        self.next_run_id = self.next_run_id.wrapping_add(1);
        let run_id = self.next_run_id;
        self.load_state = LoadState::LoadingCore {
            run_id,
            pending_quick_check: None,
        };
        self.scroll_offset = 0;
        self.content_line_count = None;
        self.visible_rows = None;
        run_id
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.load_state, LoadState::LoadingCore { .. })
    }

    pub fn is_quick_check_running(&self) -> bool {
        matches!(
            self.load_state,
            LoadState::Loaded {
                quick_check_running: true,
                ..
            }
        )
    }

    pub fn begin_quick_check(&mut self) -> Option<u64> {
        match &mut self.load_state {
            LoadState::Loaded {
                run_id,
                snapshot,
                quick_check_running,
            } if !*quick_check_running => {
                snapshot.quick_check = DiagnosticField::Pending;
                *quick_check_running = true;
                Some(*run_id)
            }
            LoadState::Idle | LoadState::LoadingCore { .. } | LoadState::Loaded { .. } => None,
        }
    }

    pub fn is_current_run(&self, run_id: u64) -> bool {
        match &self.load_state {
            LoadState::Idle => false,
            LoadState::LoadingCore {
                run_id: current, ..
            }
            | LoadState::Loaded {
                run_id: current, ..
            } => *current == run_id,
        }
    }

    pub fn set_core_loaded(&mut self, run_id: u64, mut snapshot: SqliteDiagnosticsSnapshot) {
        if !matches!(
            self.load_state,
            LoadState::LoadingCore {
                run_id: current, ..
            } if current == run_id
        ) {
            return;
        }

        let pending_quick_check = if let LoadState::LoadingCore {
            pending_quick_check,
            ..
        } = &mut self.load_state
        {
            pending_quick_check.take()
        } else {
            None
        };

        let quick_check_running = false;
        if let Some(quick_check) = pending_quick_check {
            snapshot.quick_check = quick_check;
        }

        self.load_state = LoadState::Loaded {
            run_id,
            snapshot: Box::new(snapshot),
            quick_check_running,
        };
    }

    pub fn set_quick_check_loaded(&mut self, run_id: u64, quick_check: DiagnosticField) {
        if !self.is_current_run(run_id) {
            return;
        }
        match &mut self.load_state {
            LoadState::LoadingCore {
                pending_quick_check,
                ..
            } => {
                *pending_quick_check = Some(quick_check);
            }
            LoadState::Loaded {
                snapshot,
                quick_check_running,
                ..
            } => {
                snapshot.quick_check = quick_check;
                *quick_check_running = false;
            }
            LoadState::Idle => {}
        }
    }

    pub fn set_loaded(&mut self, run_id: u64, snapshot: SqliteDiagnosticsSnapshot) {
        if self.is_current_run(run_id) || matches!(self.load_state, LoadState::Idle) {
            self.load_state = LoadState::Loaded {
                run_id,
                snapshot: Box::new(snapshot),
                quick_check_running: false,
            };
        }
    }

    pub fn snapshot(&self) -> Option<&SqliteDiagnosticsSnapshot> {
        match &self.load_state {
            LoadState::Loaded { snapshot, .. } => Some(snapshot),
            LoadState::Idle | LoadState::LoadingCore { .. } => None,
        }
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        if self.scroll_offset < self.max_scroll() {
            self.scroll_offset += 1;
        }
    }

    pub fn clear(&mut self) {
        self.load_state = LoadState::Idle;
        self.scroll_offset = 0;
        self.content_line_count = None;
        self.visible_rows = None;
    }

    pub fn max_scroll(&self) -> usize {
        match (self.content_line_count, self.visible_rows) {
            (Some(content), Some(visible)) => content.saturating_sub(visible),
            _ => 0,
        }
    }

    pub fn apply_viewport_metrics(&mut self, content_line_count: usize, visible_rows: usize) {
        self.content_line_count = Some(content_line_count);
        self.visible_rows = Some(visible_rows);
        self.scroll_offset = self.scroll_offset.min(self.max_scroll());
    }

    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }
}

pub fn display_rows(snapshot: &SqliteDiagnosticsSnapshot) -> Vec<DiagnosticDisplayRow> {
    [
        (DiagnosticFieldKind::DbFile, snapshot.db_file.display()),
        (
            DiagnosticFieldKind::SqliteVersion,
            snapshot.sqlite_version.display(),
        ),
        (
            DiagnosticFieldKind::ForeignKeys,
            snapshot.foreign_keys.display(),
        ),
        (
            DiagnosticFieldKind::JournalMode,
            snapshot.journal_mode.display(),
        ),
        (
            DiagnosticFieldKind::QueryOnly,
            snapshot.query_only.display(),
        ),
        (
            DiagnosticFieldKind::BusyTimeout,
            snapshot.busy_timeout.display(),
        ),
        (
            DiagnosticFieldKind::DatabaseList,
            snapshot.database_list.display(),
        ),
        (
            DiagnosticFieldKind::QuickCheck,
            snapshot.quick_check.display(),
        ),
    ]
    .into_iter()
    .map(|(kind, value)| DiagnosticDisplayRow { kind, value })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DiagnosticField;

    #[test]
    fn begin_fetch_assigns_monotonic_run_id() {
        let mut state = SqliteDiagnosticsState::default();

        let first = state.begin_fetch();
        let second = state.begin_fetch();

        assert_eq!(first, 1);
        assert_eq!(second, 2);
        assert!(state.is_loading());
    }

    #[test]
    fn stale_run_does_not_replace_loaded_snapshot() {
        let mut state = SqliteDiagnosticsState::default();
        let run_id = state.begin_fetch();
        state.set_loaded(
            run_id,
            SqliteDiagnosticsSnapshot {
                sqlite_version: DiagnosticField::ok("3.45.0"),
                ..Default::default()
            },
        );

        state.set_loaded(
            run_id + 1,
            SqliteDiagnosticsSnapshot {
                sqlite_version: DiagnosticField::ok("9.9.9"),
                ..Default::default()
            },
        );

        assert_eq!(
            state.snapshot().unwrap().sqlite_version.ok_value(),
            Some("3.45.0")
        );
    }

    #[test]
    fn core_loaded_leaves_quick_check_idle() {
        let mut state = SqliteDiagnosticsState::default();
        let run_id = state.begin_fetch();
        state.set_core_loaded(
            run_id,
            SqliteDiagnosticsSnapshot {
                sqlite_version: DiagnosticField::ok("3.45.0"),
                quick_check: DiagnosticField::Pending,
                ..Default::default()
            },
        );

        assert!(!state.is_loading());
        assert!(!state.is_quick_check_running());
        assert!(state.snapshot().unwrap().quick_check.is_pending());
    }

    #[test]
    fn begin_quick_check_marks_loaded_snapshot_running() {
        let mut state = SqliteDiagnosticsState::default();
        let run_id = state.begin_fetch();
        state.set_core_loaded(run_id, SqliteDiagnosticsSnapshot::default());

        let quick_check_run_id = state.begin_quick_check();

        assert_eq!(quick_check_run_id, Some(run_id));
        assert!(state.is_quick_check_running());
    }

    #[test]
    fn quick_check_loaded_clears_running_flag() {
        let mut state = SqliteDiagnosticsState::default();
        let run_id = state.begin_fetch();
        state.set_core_loaded(run_id, SqliteDiagnosticsSnapshot::default());
        state.begin_quick_check();
        state.set_quick_check_loaded(run_id, DiagnosticField::ok("ok"));

        assert!(!state.is_quick_check_running());
    }

    #[test]
    fn quick_check_before_core_is_applied_when_core_arrives() {
        let mut state = SqliteDiagnosticsState::default();
        let run_id = state.begin_fetch();
        state.set_quick_check_loaded(run_id, DiagnosticField::ok("ok"));

        state.set_core_loaded(
            run_id,
            SqliteDiagnosticsSnapshot {
                sqlite_version: DiagnosticField::ok("3.45.0"),
                ..Default::default()
            },
        );

        assert!(!state.is_quick_check_running());
        assert_eq!(state.snapshot().unwrap().quick_check.ok_value(), Some("ok"));
    }

    #[test]
    fn max_scroll_is_zero_when_content_fits_viewport() {
        let mut state = SqliteDiagnosticsState::default();
        state.apply_viewport_metrics(5, 10);

        assert_eq!(state.max_scroll(), 0);
        state.scroll_down();
        assert_eq!(state.scroll_offset(), 0);
    }

    #[test]
    fn max_scroll_clamps_to_content_minus_visible_rows() {
        let mut state = SqliteDiagnosticsState::default();
        state.apply_viewport_metrics(12, 5);

        assert_eq!(state.max_scroll(), 7);
        state.scroll_offset = 99;
        state.apply_viewport_metrics(12, 5);
        assert_eq!(state.scroll_offset(), 7);
    }
}
