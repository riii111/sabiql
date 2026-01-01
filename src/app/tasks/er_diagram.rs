use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::app::action::Action;
use crate::infra::export::{DotExporter, ErTableInfo};

pub fn spawn_er_diagram_task(
    tables: Vec<ErTableInfo>,
    total_tables: usize,
    cache_dir: PathBuf,
    tx: mpsc::Sender<Action>,
) {
    let table_count = tables.len();
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            let dot_content = DotExporter::generate_full_dot(&tables);
            DotExporter::export_dot_and_open(&dot_content, "er_full.dot", &cache_dir)
        })
        .await;

        match result {
            Ok(Ok(path)) => {
                let _ = tx
                    .send(Action::ErDiagramOpened {
                        path: path.display().to_string(),
                        table_count,
                        total_tables,
                    })
                    .await;
            }
            Ok(Err(e)) => {
                let _ = tx.send(Action::ErDiagramFailed(e.to_string())).await;
            }
            Err(e) => {
                let _ = tx
                    .send(Action::ErDiagramFailed(format!("Task panicked: {}", e)))
                    .await;
            }
        }
    });
}

pub fn write_er_failure_log_blocking(failed_tables: Vec<(String, String)>, cache_dir: PathBuf) {
    use std::io::Write;

    let log_path = cache_dir.join("er_diagram.log");
    let file = std::fs::File::create(&log_path);

    if let Ok(mut file) = file {
        let _ = writeln!(file, "ER Diagram Generation Failed");
        let _ = writeln!(file, "Timestamp: {:?}", std::time::SystemTime::now());
        let _ = writeln!(file);
        let _ = writeln!(file, "Failed tables ({}):", failed_tables.len());

        for (table, error) in &failed_tables {
            let _ = writeln!(file, "  - {}: {}", table, error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod write_er_failure_log {
        use super::*;

        #[test]
        fn writes_failure_details_to_log_file() {
            let temp_dir = tempfile::tempdir().unwrap();
            let failed_tables = vec![
                ("public.users".to_string(), "connection timeout".to_string()),
                ("public.orders".to_string(), "permission denied".to_string()),
            ];

            write_er_failure_log_blocking(failed_tables, temp_dir.path().to_path_buf());

            let log_path = temp_dir.path().join("er_diagram.log");
            assert!(log_path.exists());

            let content = std::fs::read_to_string(&log_path).unwrap();
            assert!(content.contains("ER Diagram Generation Failed"));
            assert!(content.contains("Failed tables (2):"));
            assert!(content.contains("public.users: connection timeout"));
            assert!(content.contains("public.orders: permission denied"));
        }

        #[test]
        fn writes_empty_list_when_no_failures() {
            let temp_dir = tempfile::tempdir().unwrap();
            let failed_tables: Vec<(String, String)> = vec![];

            write_er_failure_log_blocking(failed_tables, temp_dir.path().to_path_buf());

            let log_path = temp_dir.path().join("er_diagram.log");
            assert!(log_path.exists());

            let content = std::fs::read_to_string(&log_path).unwrap();
            assert!(content.contains("Failed tables (0):"));
        }

        #[test]
        fn includes_timestamp_in_log() {
            let temp_dir = tempfile::tempdir().unwrap();
            let failed_tables = vec![("public.test".to_string(), "error".to_string())];

            write_er_failure_log_blocking(failed_tables, temp_dir.path().to_path_buf());

            let log_path = temp_dir.path().join("er_diagram.log");
            let content = std::fs::read_to_string(&log_path).unwrap();
            assert!(content.contains("Timestamp:"));
        }
    }
}
