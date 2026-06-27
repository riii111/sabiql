use crate::domain::SqliteDiagnosticsSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum LoadState {
    #[default]
    Idle,
    Loading {
        run_id: u64,
    },
    Loaded {
        run_id: u64,
        snapshot: Box<SqliteDiagnosticsSnapshot>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct SqliteDiagnosticsState {
    next_run_id: u64,
    load_state: LoadState,
    scroll_offset: usize,
}

impl SqliteDiagnosticsState {
    pub fn begin_fetch(&mut self) -> u64 {
        self.next_run_id = self.next_run_id.wrapping_add(1);
        let run_id = self.next_run_id;
        self.load_state = LoadState::Loading { run_id };
        self.scroll_offset = 0;
        run_id
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.load_state, LoadState::Loading { .. })
    }

    pub fn is_current_run(&self, run_id: u64) -> bool {
        match &self.load_state {
            LoadState::Idle => false,
            LoadState::Loading { run_id: current }
            | LoadState::Loaded {
                run_id: current, ..
            } => *current == run_id,
        }
    }

    pub fn set_loaded(&mut self, run_id: u64, snapshot: SqliteDiagnosticsSnapshot) {
        if self.is_current_run(run_id) {
            self.load_state = LoadState::Loaded {
                run_id,
                snapshot: Box::new(snapshot),
            };
        }
    }

    pub fn snapshot(&self) -> Option<&SqliteDiagnosticsSnapshot> {
        match &self.load_state {
            LoadState::Loaded { snapshot, .. } => Some(snapshot),
            LoadState::Idle | LoadState::Loading { .. } => None,
        }
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self, max_scroll: usize) {
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }

    pub fn clear(&mut self) {
        self.load_state = LoadState::Idle;
        self.scroll_offset = 0;
    }

    pub fn line_count(&self) -> usize {
        self.snapshot()
            .map_or(0, |snapshot| display_lines(snapshot).len())
    }
}

pub fn display_lines(snapshot: &SqliteDiagnosticsSnapshot) -> Vec<(String, String)> {
    vec![
        ("Database file".to_string(), snapshot.db_file.display()),
        (
            "SQLite version".to_string(),
            snapshot.sqlite_version.display(),
        ),
        ("Foreign keys".to_string(), snapshot.foreign_keys.display()),
        ("Journal mode".to_string(), snapshot.journal_mode.display()),
        ("Query only".to_string(), snapshot.query_only.display()),
        (
            "Busy timeout (ms)".to_string(),
            snapshot.busy_timeout.display(),
        ),
        (
            "Attached databases".to_string(),
            snapshot.database_list.display(),
        ),
        ("Quick check".to_string(), snapshot.quick_check.display()),
    ]
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
            state.snapshot().unwrap().sqlite_version.value.as_deref(),
            Some("3.45.0")
        );
    }
}
