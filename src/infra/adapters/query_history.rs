use std::path::PathBuf;

use async_trait::async_trait;

use crate::app::ports::QueryHistoryStore;
use crate::domain::connection::ConnectionId;
use crate::domain::query_history::QueryHistoryEntry;
use crate::infra::config::cache::get_cache_dir;

const MAX_HISTORY_ENTRIES: usize = 1000;

pub struct FileQueryHistoryStore;

impl FileQueryHistoryStore {
    fn history_path(project_name: &str, connection_id: &ConnectionId) -> Result<PathBuf, String> {
        let cache_dir = get_cache_dir(project_name).map_err(|e| e.to_string())?;
        let history_dir = cache_dir.join("history");
        if !history_dir.exists() {
            std::fs::create_dir_all(&history_dir).map_err(|e| e.to_string())?;
        }
        Ok(history_dir.join(format!("{}.jsonl", connection_id)))
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
        let path = Self::history_path(project_name, connection_id)?;

        let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;

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
    }

    async fn load(
        &self,
        project_name: &str,
        connection_id: &ConnectionId,
    ) -> Result<Vec<QueryHistoryEntry>, String> {
        let path = Self::history_path(project_name, connection_id)?;

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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    struct TempQueryHistoryStore {
        base_dir: PathBuf,
    }

    impl TempQueryHistoryStore {
        fn new(base_dir: PathBuf) -> Self {
            Self { base_dir }
        }

        fn history_path(&self, connection_id: &ConnectionId) -> PathBuf {
            let history_dir = self.base_dir.join("history");
            if !history_dir.exists() {
                std::fs::create_dir_all(&history_dir).unwrap();
            }
            history_dir.join(format!("{}.jsonl", connection_id))
        }
    }

    #[async_trait]
    impl QueryHistoryStore for TempQueryHistoryStore {
        async fn append(
            &self,
            _project_name: &str,
            connection_id: &ConnectionId,
            entry: &QueryHistoryEntry,
        ) -> Result<(), String> {
            let path = self.history_path(connection_id);
            let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;

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

            let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let lines: Vec<&str> = content.lines().collect();
            if lines.len() > MAX_HISTORY_ENTRIES {
                let trimmed = &lines[lines.len() - MAX_HISTORY_ENTRIES..];
                let tmp_path = path.with_extension("jsonl.tmp");
                std::fs::write(&tmp_path, trimmed.join("\n") + "\n").map_err(|e| e.to_string())?;
                std::fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
            }

            Ok(())
        }

        async fn load(
            &self,
            _project_name: &str,
            connection_id: &ConnectionId,
        ) -> Result<Vec<QueryHistoryEntry>, String> {
            let path = self.history_path(connection_id);

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
        }
    }

    fn make_entry(query: &str) -> QueryHistoryEntry {
        QueryHistoryEntry::new(
            query.to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-conn"),
        )
    }

    #[tokio::test]
    async fn append_creates_file_and_writes_entry() {
        let tmp = TempDir::new().unwrap();
        let store = TempQueryHistoryStore::new(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        let entry = make_entry("SELECT 1");
        store.append("test", &conn_id, &entry).await.unwrap();

        let path = store.history_path(&conn_id);
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("SELECT 1"));
    }

    #[tokio::test]
    async fn load_returns_entries_in_order() {
        let tmp = TempDir::new().unwrap();
        let store = TempQueryHistoryStore::new(tmp.path().to_path_buf());
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
        let store = TempQueryHistoryStore::new(tmp.path().to_path_buf());
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
        let store = TempQueryHistoryStore::new(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("nonexistent");

        let entries = store.load("test", &conn_id).await.unwrap();

        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn malformed_lines_are_skipped() {
        let tmp = TempDir::new().unwrap();
        let store = TempQueryHistoryStore::new(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        // Write a valid entry
        store
            .append("test", &conn_id, &make_entry("SELECT 1"))
            .await
            .unwrap();

        // Manually append a malformed line
        let path = store.history_path(&conn_id);
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
        let store = TempQueryHistoryStore::new(tmp.path().to_path_buf());
        let conn_id = ConnectionId::from_string("test-conn");

        // Write some entries
        for i in 0..5 {
            store
                .append("test", &conn_id, &make_entry(&format!("SELECT {}", i)))
                .await
                .unwrap();
        }

        // Verify all entries are preserved
        let entries = store.load("test", &conn_id).await.unwrap();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].query, "SELECT 0");
        assert_eq!(entries[4].query, "SELECT 4");
    }
}
