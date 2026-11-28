use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::app::ports::outbound::{QueryHistoryError, QueryHistoryStore};
use crate::config::{CacheDirError, get_cache_dir};
use crate::domain::query_history::{QueryHistoryEntry, QueryHistoryScope};

const MAX_HISTORY_ENTRIES: usize = 1000;

impl From<CacheDirError> for QueryHistoryError {
    fn from(error: CacheDirError) -> Self {
        match error {
            CacheDirError::BaseDirUnavailable => Self::MissingCacheDir,
            CacheDirError::Io(error) => error.into(),
        }
    }
}

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

pub struct FileQueryHistoryStore {
    base_dir: Option<PathBuf>,
}

impl FileQueryHistoryStore {
    #[allow(
        clippy::new_without_default,
        reason = "new() is the only default construction API"
    )]
    pub fn new() -> Self {
        Self { base_dir: None }
    }

    fn resolve_history_dir(&self, project_name: &str) -> Result<PathBuf, QueryHistoryError> {
        if let Some(base) = &self.base_dir {
            Ok(base.join("history"))
        } else {
            let cache_dir = get_cache_dir(project_name)?;
            Ok(cache_dir.join("history"))
        }
    }
}

#[async_trait]
impl QueryHistoryStore for FileQueryHistoryStore {
    async fn append(
        &self,
        project_name: &str,
        scope: &QueryHistoryScope,
        entry: &QueryHistoryEntry,
    ) -> Result<(), QueryHistoryError> {
        let history_dir = self.resolve_history_dir(project_name)?;
        let path = history_dir.join(format!("{}.jsonl", scope.connection_id));
        let line = serde_json::to_string(entry)?;

        tokio::task::spawn_blocking(move || {
            append_entry(&path, &history_dir, &line)?;
            // Trim is best-effort: auxiliary data, next successful append will retry.
            if let Err(_err) = trim_if_exceeded(&path, MAX_HISTORY_ENTRIES) {}
            Ok(())
        })
        .await?
    }

    async fn load(
        &self,
        project_name: &str,
        scope: &QueryHistoryScope,
    ) -> Result<Vec<QueryHistoryEntry>, QueryHistoryError> {
        let history_dir = self.resolve_history_dir(project_name)?;
        let path = history_dir.join(format!("{}.jsonl", scope.connection_id));
        let database = scope.database.clone();

        tokio::task::spawn_blocking(move || {
            if !path.exists() {
                return Ok(Vec::new());
            }

            let content = std::fs::read_to_string(&path)?;
            let entries: Vec<QueryHistoryEntry> = content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| serde_json::from_str(line).ok())
                .filter(|entry: &QueryHistoryEntry| entry.database == database)
                .collect();

            Ok(entries)
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::ConnectionId;
    use crate::domain::query_history::QueryResultStatus;
    use tempfile::TempDir;

    impl FileQueryHistoryStore {
        fn with_base_dir(base_dir: PathBuf) -> Self {
            Self {
                base_dir: Some(base_dir),
            }
        }
    }

    fn make_entry(query: &str) -> QueryHistoryEntry {
        QueryHistoryEntry::new_with_database(
            query.to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-conn"),
            None,
            QueryResultStatus::Success,
            None,
        )
    }

    fn scope(connection_id: &ConnectionId) -> QueryHistoryScope {
        QueryHistoryScope::new(connection_id.clone(), None)
    }

    fn database_scope(connection_id: &ConnectionId, database: &str) -> QueryHistoryScope {
        QueryHistoryScope::new(connection_id.clone(), Some(database.to_string()))
    }

    #[tokio::test]
    async fn append_and_load_succeed_for_cli_sqlite_connection_id() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("cli-sqlite-test");

        assert!(!conn_id.as_str().contains('/'));

        store
            .append("test", &scope(&conn_id), &make_entry("SELECT 1"))
            .await
            .unwrap();

        let history_dir = tmp.path().join("history");
        let path = history_dir.join(format!("{conn_id}.jsonl"));
        assert!(path.is_file());

        let entries = store.load("test", &scope(&conn_id)).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].query, "SELECT 1");
    }

    #[tokio::test]
    async fn append_creates_file_and_writes_entry() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        let entry = make_entry("SELECT 1");
        store
            .append("test", &scope(&conn_id), &entry)
            .await
            .unwrap();

        let history_dir = tmp.path().join("history");
        let path = history_dir.join(format!("{conn_id}.jsonl"));
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("SELECT 1"));
    }

    #[tokio::test]
    async fn load_returns_entries_in_order() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        store
            .append("test", &scope(&conn_id), &make_entry("SELECT 1"))
            .await
            .unwrap();
        store
            .append("test", &scope(&conn_id), &make_entry("SELECT 2"))
            .await
            .unwrap();
        store
            .append("test", &scope(&conn_id), &make_entry("SELECT 3"))
            .await
            .unwrap();

        let entries = store.load("test", &scope(&conn_id)).await.unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].query, "SELECT 1");
        assert_eq!(entries[1].query, "SELECT 2");
        assert_eq!(entries[2].query, "SELECT 3");
    }

    #[tokio::test]
    async fn load_filters_entries_by_database_scope() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");
        let app_scope = database_scope(&conn_id, "app");
        let analytics_scope = database_scope(&conn_id, "analytics");

        store
            .append(
                "test",
                &app_scope,
                &QueryHistoryEntry::new_with_database(
                    "SELECT app".to_string(),
                    "2026-03-13T12:00:00Z".to_string(),
                    conn_id.clone(),
                    Some("app".to_string()),
                    QueryResultStatus::Success,
                    None,
                ),
            )
            .await
            .unwrap();
        store
            .append(
                "test",
                &analytics_scope,
                &QueryHistoryEntry::new_with_database(
                    "SELECT analytics".to_string(),
                    "2026-03-13T12:01:00Z".to_string(),
                    conn_id.clone(),
                    Some("analytics".to_string()),
                    QueryResultStatus::Success,
                    None,
                ),
            )
            .await
            .unwrap();

        let app_entries = store.load("test", &app_scope).await.unwrap();
        let analytics_entries = store.load("test", &analytics_scope).await.unwrap();

        assert_eq!(
            app_entries
                .iter()
                .map(|entry| entry.query.as_str())
                .collect::<Vec<_>>(),
            ["SELECT app"]
        );
        assert_eq!(
            analytics_entries
                .iter()
                .map(|entry| entry.query.as_str())
                .collect::<Vec<_>>(),
            ["SELECT analytics"]
        );
    }

    #[tokio::test]
    async fn load_nonexistent_file_returns_empty_vec() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("nonexistent");

        let entries = store.load("test", &scope(&conn_id)).await.unwrap();

        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn malformed_lines_are_skipped() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        // Write a valid entry
        store
            .append("test", &scope(&conn_id), &make_entry("SELECT 1"))
            .await
            .unwrap();

        // Manually append a malformed line
        let history_dir = tmp.path().join("history");
        let path = history_dir.join(format!("{conn_id}.jsonl"));
        use std::fs::OpenOptions;
        use std::io::Write;
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{invalid json}}").unwrap();

        // Write another valid entry
        store
            .append("test", &scope(&conn_id), &make_entry("SELECT 2"))
            .await
            .unwrap();

        let entries = store.load("test", &scope(&conn_id)).await.unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].query, "SELECT 1");
        assert_eq!(entries[1].query, "SELECT 2");
    }

    #[test]
    fn trim_keeps_the_newest_lines_in_order_only_above_limit() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("history.jsonl");
        let limit = 2;

        for content in ["first", "first\nsecond"] {
            std::fs::write(&path, content).unwrap();
            trim_if_exceeded(&path, limit).unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        }

        std::fs::write(&path, "first\nsecond\nthird\nfourth\n").unwrap();
        trim_if_exceeded(&path, limit).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "third\nfourth\n");
    }

    #[tokio::test]
    async fn append_keeps_new_entries_when_trim_fails_and_retries_on_next_append() {
        use std::fmt::Write as _;

        let expected_limit = 1_000;
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let scope = scope(&ConnectionId::from_string("test-conn"));
        let history_dir = tmp.path().join("history");
        let path = history_dir.join("test-conn.jsonl");
        let tmp_path = path.with_extension("jsonl.tmp");
        let mut content = String::new();
        for i in 0..expected_limit {
            let entry = make_entry(&format!("SELECT {i}"));
            writeln!(content, "{}", serde_json::to_string(&entry).unwrap()).unwrap();
        }
        std::fs::create_dir_all(&tmp_path).unwrap();
        std::fs::write(&path, content).unwrap();

        store
            .append("test", &scope, &make_entry("SELECT first_append"))
            .await
            .unwrap();

        let entries = store.load("test", &scope).await.unwrap();
        assert_eq!(entries.len(), expected_limit + 1);
        assert_eq!(entries.last().unwrap().query, "SELECT first_append");

        std::fs::remove_dir(&tmp_path).unwrap();
        store
            .append("test", &scope, &make_entry("SELECT second_append"))
            .await
            .unwrap();

        let entries = store.load("test", &scope).await.unwrap();
        assert_eq!(entries.len(), expected_limit);
        assert_eq!(entries[0].query, "SELECT 2");
        assert_eq!(entries[expected_limit - 2].query, "SELECT first_append");
        assert_eq!(entries.last().unwrap().query, "SELECT second_append");
    }

    #[tokio::test]
    async fn load_entries_with_affected_rows_and_malformed_lines() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        let history_dir = tmp.path().join("history");
        std::fs::create_dir_all(&history_dir).unwrap();
        let path = history_dir.join(format!("{conn_id}.jsonl"));

        use std::io::Write;
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, r#"{{"query":"SELECT 1","executed_at":"2026-03-13T12:00:00Z","connection_id":"test-conn","result_status":"Success","affected_rows":null}}"#).unwrap();
        writeln!(file, r#"{{"query":"UPDATE t SET x=1","executed_at":"2026-03-13T12:01:00Z","connection_id":"test-conn","result_status":"Success","affected_rows":5}}"#).unwrap();
        // Malformed line should be skipped
        writeln!(file, "not valid json").unwrap();

        let entries = store.load("test", &scope(&conn_id)).await.unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].query, "SELECT 1");
        assert_eq!(entries[0].result_status, QueryResultStatus::Success);
        assert_eq!(entries[0].affected_rows, None);
        assert_eq!(entries[1].query, "UPDATE t SET x=1");
        assert_eq!(entries[1].result_status, QueryResultStatus::Success);
        assert_eq!(entries[1].affected_rows, Some(5));
    }
}
