use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::app::ports::QueryHistoryStore;
use crate::domain::connection::ConnectionId;
use crate::domain::query_history::QueryHistoryEntry;
use crate::infra::config::cache::get_cache_dir;

const MAX_HISTORY_ENTRIES: usize = 1000;

fn append_entry(path: &Path, dir: &Path, line: &str) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::Write;

    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    writeln!(file, "{}", line).map_err(|e| e.to_string())
}

fn trim_if_exceeded(path: &Path, max: usize) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() > max {
        let trimmed = &lines[lines.len() - max..];
        let tmp_path = path.with_extension("jsonl.tmp");
        std::fs::write(&tmp_path, trimmed.join("\n") + "\n").map_err(|e| e.to_string())?;
        std::fs::rename(&tmp_path, path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub struct FileQueryHistoryStore {
    base_dir: Option<PathBuf>,
}

impl Default for FileQueryHistoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl FileQueryHistoryStore {
    pub fn new() -> Self {
        Self { base_dir: None }
    }

    #[cfg(test)]
    fn with_base_dir(base_dir: PathBuf) -> Self {
        Self {
            base_dir: Some(base_dir),
        }
    }

    fn resolve_history_dir(&self, project_name: &str) -> Result<PathBuf, String> {
        if let Some(base) = &self.base_dir {
            Ok(base.join("history"))
        } else {
            let cache_dir = get_cache_dir(project_name).map_err(|e| e.to_string())?;
            Ok(cache_dir.join("history"))
        }
    }
}

#[async_trait]
impl QueryHistoryStore for FileQueryHistoryStore {
    async fn append(
        &self,
        project_name: &str,
        connection_id: &ConnectionId,
        entry: &QueryHistoryEntry,
    ) -> Result<(), String> {
        let history_dir = self.resolve_history_dir(project_name)?;
        let path = history_dir.join(format!("{}.jsonl", connection_id));
        let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;

        tokio::task::spawn_blocking(move || {
            append_entry(&path, &history_dir, &line)?;
            // Trim is best-effort: auxiliary data, next successful append will retry.
            if let Err(_err) = trim_if_exceeded(&path, MAX_HISTORY_ENTRIES) {}
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())?
    }

    async fn load(
        &self,
        project_name: &str,
        connection_id: &ConnectionId,
    ) -> Result<Vec<QueryHistoryEntry>, String> {
        let history_dir = self.resolve_history_dir(project_name)?;
        let path = history_dir.join(format!("{}.jsonl", connection_id));

        tokio::task::spawn_blocking(move || {
            if !path.exists() {
                return Ok(Vec::new());
            }

            let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let entries: Vec<QueryHistoryEntry> = content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();

            Ok(entries)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_entry(query: &str) -> QueryHistoryEntry {
        use crate::domain::query_history::QueryResultStatus;
        QueryHistoryEntry::new(
            query.to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-conn"),
            QueryResultStatus::Success,
            None,
        )
    }

    #[tokio::test]
    async fn append_creates_file_and_writes_entry() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        let entry = make_entry("SELECT 1");
        store.append("test", &conn_id, &entry).await.unwrap();

        let history_dir = tmp.path().join("history");
        let path = history_dir.join(format!("{}.jsonl", conn_id));
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
            .append("test", &conn_id, &make_entry("SELECT 1"))
            .await
            .unwrap();
        store
            .append("test", &conn_id, &make_entry("SELECT 2"))
            .await
            .unwrap();
        store
            .append("test", &conn_id, &make_entry("SELECT 3"))
            .await
            .unwrap();

        let entries = store.load("test", &conn_id).await.unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].query, "SELECT 1");
        assert_eq!(entries[1].query, "SELECT 2");
        assert_eq!(entries[2].query, "SELECT 3");
    }

    #[tokio::test]
    async fn append_trims_to_1000_when_exceeded() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        // Write 1001 entries
        for i in 0..1001 {
            store
                .append("test", &conn_id, &make_entry(&format!("SELECT {}", i)))
                .await
                .unwrap();
        }

        let entries = store.load("test", &conn_id).await.unwrap();

        assert_eq!(entries.len(), MAX_HISTORY_ENTRIES);
        // Oldest entry (SELECT 0) should be trimmed, newest (SELECT 1000) should remain
        assert_eq!(entries[0].query, "SELECT 1");
        assert_eq!(entries[MAX_HISTORY_ENTRIES - 1].query, "SELECT 1000");
    }

    #[tokio::test]
    async fn load_nonexistent_file_returns_empty_vec() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("nonexistent");

        let entries = store.load("test", &conn_id).await.unwrap();

        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn malformed_lines_are_skipped() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        // Write a valid entry
        store
            .append("test", &conn_id, &make_entry("SELECT 1"))
            .await
            .unwrap();

        // Manually append a malformed line
        let history_dir = tmp.path().join("history");
        let path = history_dir.join(format!("{}.jsonl", conn_id));
        use std::fs::OpenOptions;
        use std::io::Write;
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{invalid json}}").unwrap();

        // Write another valid entry
        store
            .append("test", &conn_id, &make_entry("SELECT 2"))
            .await
            .unwrap();

        let entries = store.load("test", &conn_id).await.unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].query, "SELECT 1");
        assert_eq!(entries[1].query, "SELECT 2");
    }

    #[tokio::test]
    async fn below_limit_entries_are_preserved_without_trim() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        for i in 0..5 {
            store
                .append("test", &conn_id, &make_entry(&format!("SELECT {}", i)))
                .await
                .unwrap();
        }

        let entries = store.load("test", &conn_id).await.unwrap();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].query, "SELECT 0");
        assert_eq!(entries[4].query, "SELECT 4");
    }

    #[tokio::test]
    async fn trim_does_not_trigger_at_exact_limit() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        for i in 0..MAX_HISTORY_ENTRIES {
            store
                .append("test", &conn_id, &make_entry(&format!("SELECT {}", i)))
                .await
                .unwrap();
        }

        let entries = store.load("test", &conn_id).await.unwrap();
        assert_eq!(entries.len(), MAX_HISTORY_ENTRIES);
        assert_eq!(entries[0].query, "SELECT 0");
        assert_eq!(
            entries[MAX_HISTORY_ENTRIES - 1].query,
            format!("SELECT {}", MAX_HISTORY_ENTRIES - 1)
        );
    }

    #[tokio::test]
    async fn multiple_appends_beyond_limit_preserves_latest() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        let total = MAX_HISTORY_ENTRIES + 5;
        for i in 0..total {
            store
                .append("test", &conn_id, &make_entry(&format!("SELECT {}", i)))
                .await
                .unwrap();
        }

        let entries = store.load("test", &conn_id).await.unwrap();
        assert_eq!(entries.len(), MAX_HISTORY_ENTRIES);
        // Oldest 5 entries (0..5) should be trimmed
        assert_eq!(entries[0].query, "SELECT 5");
        assert_eq!(
            entries[MAX_HISTORY_ENTRIES - 1].query,
            format!("SELECT {}", total - 1)
        );
    }

    #[tokio::test]
    async fn append_succeeds_even_when_trim_would_fail() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        // Fill to just above the limit so trim fires on next append
        for i in 0..MAX_HISTORY_ENTRIES {
            store
                .append("test", &conn_id, &make_entry(&format!("SELECT {}", i)))
                .await
                .unwrap();
        }

        // Make the .tmp path a directory so fs::write in trim_if_exceeded fails
        let history_dir = tmp.path().join("history");
        let tmp_path = history_dir.join(format!("{}.jsonl.tmp", conn_id));
        std::fs::create_dir_all(&tmp_path).unwrap();

        // append should still succeed (trim failure is best-effort)
        let result = store
            .append("test", &conn_id, &make_entry("SELECT final"))
            .await;
        assert!(result.is_ok());

        // The entry was written even though trim failed
        let entries = store.load("test", &conn_id).await.unwrap();
        assert!(entries.len() > MAX_HISTORY_ENTRIES);
        assert_eq!(entries.last().unwrap().query, "SELECT final");
    }

    #[tokio::test]
    async fn load_entries_with_affected_rows_and_malformed_lines() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        let history_dir = tmp.path().join("history");
        std::fs::create_dir_all(&history_dir).unwrap();
        let path = history_dir.join(format!("{}.jsonl", conn_id));

        use std::io::Write;
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, r#"{{"query":"SELECT 1","executed_at":"2026-03-13T12:00:00Z","connection_id":"test-conn","result_status":"Success","affected_rows":null}}"#).unwrap();
        writeln!(file, r#"{{"query":"UPDATE t SET x=1","executed_at":"2026-03-13T12:01:00Z","connection_id":"test-conn","result_status":"Success","affected_rows":5}}"#).unwrap();
        // Malformed line should be skipped
        writeln!(file, "not valid json").unwrap();

        let entries = store.load("test", &conn_id).await.unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].query, "SELECT 1");
        assert_eq!(
            entries[0].result_status,
            crate::domain::query_history::QueryResultStatus::Success
        );
        assert_eq!(entries[0].affected_rows, None);
        assert_eq!(entries[1].query, "UPDATE t SET x=1");
        assert_eq!(
            entries[1].result_status,
            crate::domain::query_history::QueryResultStatus::Success
        );
        assert_eq!(entries[1].affected_rows, Some(5));
    }
}
