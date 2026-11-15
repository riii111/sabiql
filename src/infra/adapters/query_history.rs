use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::app::ports::outbound::{QueryHistoryError, QueryHistoryStore};
use crate::domain::query_history::{QueryHistoryEntry, QueryHistoryScope};

const MAX_HISTORY_ENTRIES: usize = 1000;

fn append_entry(path: &Path, dir: &Path, line: &str) -> Result<(), QueryHistoryError> {
    use std::fs::OpenOptions;
    use std::io::Write;

    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn trim_if_exceeded(path: &Path, max: usize) -> Result<(), QueryHistoryError> {
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() > max {
        let trimmed = &lines[lines.len() - max..];
        let tmp_path = path.with_extension("jsonl.tmp");
        std::fs::write(&tmp_path, trimmed.join("\n") + "\n")?;
        std::fs::rename(&tmp_path, path)?;
    }
    Ok(())
}

/// Reads every `*.jsonl` file in `dir`. History is auxiliary data, so a
/// missing directory, an unreadable file or a malformed line is skipped
/// rather than hiding the rest.
fn read_history_dir(dir: &Path, entries: &mut Vec<QueryHistoryEntry>) {
    let Ok(files) = std::fs::read_dir(dir) else {
        return;
    };
    for file in files.flatten() {
        let path = file.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        entries.extend(
            content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| serde_json::from_str::<QueryHistoryEntry>(line).ok()),
        );
    }
}

#[derive(Debug, Clone)]
struct HistoryDirs {
    /// `<data dir>/sabiql/query_history`, one file per history key.
    current: PathBuf,
    /// `<cache dir>/sabiql`, whose `<project>/history/<connection id>.jsonl`
    /// files hold history written by earlier versions. Read-only.
    legacy_cache_root: Option<PathBuf>,
}

impl HistoryDirs {
    fn load_all(&self) -> Vec<QueryHistoryEntry> {
        let mut entries = Vec::new();
        read_history_dir(&self.current, &mut entries);
        if let Some(Ok(projects)) = self.legacy_cache_root.as_deref().map(std::fs::read_dir) {
            for project in projects.flatten() {
                read_history_dir(&project.path().join("history"), &mut entries);
            }
        }
        // Timestamps are RFC 3339 UTC (`...Z`), so string order is time order.
        // The sort is stable, so entries sharing a second keep their file order.
        entries.sort_by(|a, b| a.executed_at.as_str().cmp(b.executed_at.as_str()));
        entries
    }
}

pub struct FileQueryHistoryStore {
    dirs: Option<HistoryDirs>,
}

impl FileQueryHistoryStore {
    #[allow(
        clippy::new_without_default,
        reason = "new() is the only default construction API"
    )]
    pub fn new() -> Self {
        Self { dirs: None }
    }

    fn resolve_dirs(&self) -> Result<HistoryDirs, QueryHistoryError> {
        if let Some(dirs) = &self.dirs {
            return Ok(dirs.clone());
        }
        let data_dir = dirs::data_dir().ok_or(QueryHistoryError::MissingDataDir)?;
        Ok(HistoryDirs {
            current: data_dir.join("sabiql").join("query_history"),
            legacy_cache_root: dirs::cache_dir().map(|cache| cache.join("sabiql")),
        })
    }
}

#[async_trait]
impl QueryHistoryStore for FileQueryHistoryStore {
    async fn append(
        &self,
        scope: &QueryHistoryScope,
        entry: &QueryHistoryEntry,
    ) -> Result<(), QueryHistoryError> {
        let history_dir = self.resolve_dirs()?.current;
        let path = history_dir.join(format!("{}.jsonl", scope.key));
        let line = serde_json::to_string(entry)?;

        tokio::task::spawn_blocking(move || {
            append_entry(&path, &history_dir, &line)?;
            // Trim is best-effort: auxiliary data, next successful append will retry.
            if let Err(_err) = trim_if_exceeded(&path, MAX_HISTORY_ENTRIES) {}
            Ok(())
        })
        .await?
    }

    async fn load(&self) -> Result<Vec<QueryHistoryEntry>, QueryHistoryError> {
        let dirs = self.resolve_dirs()?;
        Ok(tokio::task::spawn_blocking(move || dirs.load_all()).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::ConnectionId;
    use crate::domain::query_history::{QueryHistoryKey, QueryResultStatus};
    use tempfile::TempDir;

    impl FileQueryHistoryStore {
        fn in_temp_dir(tmp: &TempDir) -> Self {
            Self {
                dirs: Some(HistoryDirs {
                    current: tmp.path().join("data"),
                    legacy_cache_root: Some(tmp.path().join("cache")),
                }),
            }
        }
    }

    fn scope_for(connection_id: &str, target: &str, database: Option<&str>) -> QueryHistoryScope {
        QueryHistoryScope::new(
            ConnectionId::from_string(connection_id),
            QueryHistoryKey::for_connection_id(&ConnectionId::from_string(target)),
            database.map(str::to_owned),
        )
    }

    fn scope() -> QueryHistoryScope {
        scope_for("test-conn", "test-target", None)
    }

    fn history_path(tmp: &TempDir, scope: &QueryHistoryScope) -> PathBuf {
        tmp.path().join("data").join(format!("{}.jsonl", scope.key))
    }

    fn make_entry(query: &str) -> QueryHistoryEntry {
        entry_at(&scope(), query, "2026-03-13T12:00:00Z")
    }

    fn entry_at(scope: &QueryHistoryScope, query: &str, at: &str) -> QueryHistoryEntry {
        scope.entry(
            query.to_string(),
            at.to_string(),
            QueryResultStatus::Success,
            None,
        )
    }

    async fn append_n(store: &FileQueryHistoryStore, range: std::ops::Range<usize>) {
        for i in range {
            store
                .append(&scope(), &make_entry(&format!("SELECT {i}")))
                .await
                .unwrap();
        }
    }

    fn write_legacy_file(tmp: &TempDir, project: &str, connection_id: &str, lines: &[&str]) {
        let dir = tmp.path().join("cache").join(project).join("history");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{connection_id}.jsonl")),
            lines.join("\n") + "\n",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn append_writes_one_file_per_history_key() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::in_temp_dir(&tmp);

        store
            .append(&scope(), &make_entry("SELECT 1"))
            .await
            .unwrap();

        let content = std::fs::read_to_string(history_path(&tmp, &scope())).unwrap();
        assert!(content.contains("SELECT 1"));
        assert!(content.contains(scope().key.as_str()));
    }

    #[tokio::test]
    async fn profiles_sharing_a_key_share_a_file() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::in_temp_dir(&tmp);
        let deleted = scope_for("deleted-profile", "same-target", None);
        let readded = scope_for("readded-profile", "same-target", None);

        store
            .append(
                &deleted,
                &entry_at(&deleted, "SELECT old", "2026-03-13T12:00:00Z"),
            )
            .await
            .unwrap();
        store
            .append(
                &readded,
                &entry_at(&readded, "SELECT new", "2026-03-13T12:01:00Z"),
            )
            .await
            .unwrap();

        let entries = store.load().await.unwrap();
        assert!(entries.iter().all(|entry| readded.matches(entry)));
        assert_eq!(
            std::fs::read_to_string(history_path(&tmp, &readded))
                .unwrap()
                .lines()
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn load_merges_current_and_legacy_history_oldest_first() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::in_temp_dir(&tmp);
        let other = scope_for("other-conn", "other-target", Some("app"));

        store
            .append(
                &scope(),
                &entry_at(&scope(), "SELECT current", "2026-03-13T12:02:00Z"),
            )
            .await
            .unwrap();
        store
            .append(
                &other,
                &entry_at(&other, "SELECT other", "2026-03-13T12:00:00Z"),
            )
            .await
            .unwrap();
        write_legacy_file(
            &tmp,
            "some-project",
            "legacy-conn",
            &[
                r#"{"query":"SELECT legacy","executed_at":"2026-03-13T12:01:00Z","connection_id":"legacy-conn","result_status":"Success","affected_rows":null}"#,
            ],
        );

        let entries = store.load().await.unwrap();

        assert_eq!(
            entries.iter().map(|e| e.query.as_str()).collect::<Vec<_>>(),
            ["SELECT other", "SELECT legacy", "SELECT current"]
        );
    }

    #[tokio::test]
    async fn load_reads_legacy_entries_with_affected_rows_and_skips_malformed_lines() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::in_temp_dir(&tmp);
        write_legacy_file(
            &tmp,
            "test",
            "test-conn",
            &[
                r#"{"query":"SELECT 1","executed_at":"2026-03-13T12:00:00Z","connection_id":"test-conn","result_status":"Success","affected_rows":null}"#,
                r#"{"query":"UPDATE t SET x=1","executed_at":"2026-03-13T12:01:00Z","connection_id":"test-conn","result_status":"Success","affected_rows":5}"#,
                "not valid json",
            ],
        );

        let entries = store.load().await.unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].query, "SELECT 1");
        assert_eq!(entries[0].connection_key, None);
        assert_eq!(entries[0].affected_rows, None);
        assert_eq!(entries[1].query, "UPDATE t SET x=1");
        assert_eq!(entries[1].affected_rows, Some(5));
        assert!(
            scope().matches(&entries[0]),
            "legacy entries match by profile id"
        );
    }

    #[tokio::test]
    async fn load_returns_empty_vec_when_nothing_exists() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::in_temp_dir(&tmp);

        assert!(store.load().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn malformed_lines_are_skipped() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::in_temp_dir(&tmp);

        store
            .append(&scope(), &make_entry("SELECT 1"))
            .await
            .unwrap();
        {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(history_path(&tmp, &scope()))
                .unwrap();
            writeln!(file, "{{invalid json}}").unwrap();
        }
        store
            .append(&scope(), &make_entry("SELECT 2"))
            .await
            .unwrap();

        let entries = store.load().await.unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].query, "SELECT 1");
        assert_eq!(entries[1].query, "SELECT 2");
    }

    #[tokio::test]
    async fn append_trims_to_limit_when_exceeded() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::in_temp_dir(&tmp);

        append_n(&store, 0..MAX_HISTORY_ENTRIES + 1).await;

        let entries = store.load().await.unwrap();
        assert_eq!(entries.len(), MAX_HISTORY_ENTRIES);
        assert_eq!(entries[0].query, "SELECT 1");
        assert_eq!(
            entries[MAX_HISTORY_ENTRIES - 1].query,
            format!("SELECT {MAX_HISTORY_ENTRIES}")
        );
    }

    #[tokio::test]
    async fn trim_does_not_trigger_at_exact_limit() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::in_temp_dir(&tmp);

        append_n(&store, 0..MAX_HISTORY_ENTRIES).await;

        let entries = store.load().await.unwrap();
        assert_eq!(entries.len(), MAX_HISTORY_ENTRIES);
        assert_eq!(entries[0].query, "SELECT 0");
    }

    #[tokio::test]
    async fn trim_is_per_history_key() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::in_temp_dir(&tmp);
        let other = scope_for("other-conn", "other-target", None);

        store
            .append(
                &other,
                &entry_at(&other, "SELECT kept", "2026-03-13T11:00:00Z"),
            )
            .await
            .unwrap();
        append_n(&store, 0..MAX_HISTORY_ENTRIES + 1).await;

        let entries = store.load().await.unwrap();
        assert_eq!(entries.len(), MAX_HISTORY_ENTRIES + 1);
        assert_eq!(entries[0].query, "SELECT kept");
    }

    #[tokio::test]
    async fn append_succeeds_even_when_trim_would_fail() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::in_temp_dir(&tmp);

        append_n(&store, 0..MAX_HISTORY_ENTRIES).await;

        // Make the .tmp path a directory so fs::write in trim_if_exceeded fails
        std::fs::create_dir_all(history_path(&tmp, &scope()).with_extension("jsonl.tmp")).unwrap();

        let result = store.append(&scope(), &make_entry("SELECT final")).await;
        assert!(result.is_ok());

        let entries = store.load().await.unwrap();
        assert!(entries.len() > MAX_HISTORY_ENTRIES);
        assert_eq!(entries.last().unwrap().query, "SELECT final");
    }
}
