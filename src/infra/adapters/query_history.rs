use std::path::PathBuf;

use async_trait::async_trait;

use crate::app::ports::QueryHistoryStore;
use crate::domain::connection::ConnectionId;
use crate::domain::query_history::QueryHistoryEntry;
use crate::infra::config::cache::get_cache_dir;

const MAX_HISTORY_ENTRIES: usize = 1000;

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
            if !history_dir.exists() {
                std::fs::create_dir_all(&history_dir).map_err(|e| e.to_string())?;
            }

            use std::fs::OpenOptions;
            use std::io::Write;

            {
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(|e| e.to_string())?;
                writeln!(file, "{}", line).map_err(|e| e.to_string())?;
            }

            // Trim to MAX_HISTORY_ENTRIES if exceeded
            let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let lines: Vec<&str> = content.lines().collect();
            if lines.len() > MAX_HISTORY_ENTRIES {
                let trimmed = &lines[lines.len() - MAX_HISTORY_ENTRIES..];
                let tmp_path = path.with_extension("jsonl.tmp");
                std::fs::write(&tmp_path, trimmed.join("\n") + "\n").map_err(|e| e.to_string())?;
                std::fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
            }

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
    async fn trim_failure_preserves_existing_history() {
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

    #[tokio::test]
    async fn new_format_entries_with_unknown_fields_are_loaded() {
        let tmp = TempDir::new().unwrap();
        let store = FileQueryHistoryStore::with_base_dir(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        let history_dir = tmp.path().join("history");
        std::fs::create_dir_all(&history_dir).unwrap();
        let path = history_dir.join(format!("{}.jsonl", conn_id));

        use std::io::Write;
        let mut file = std::fs::File::create(&path).unwrap();
        // Entry with an extra unknown field
        writeln!(file, r#"{{"query":"SELECT 1","executed_at":"2026-03-13T12:00:00Z","connection_id":"test-conn","result_status":"Success","future_field":"whatever"}}"#).unwrap();

        let entries = store.load("test", &conn_id).await.unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].query, "SELECT 1");
    }
}
