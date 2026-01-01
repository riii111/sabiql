use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::app::action::Action;
use crate::app::ports::ErDiagramExporter;
use crate::domain::ErTableInfo;

pub fn spawn_er_diagram_task(
    exporter: Arc<dyn ErDiagramExporter>,
    tables: Vec<ErTableInfo>,
    total_tables: usize,
    cache_dir: PathBuf,
    tx: mpsc::Sender<Action>,
) {
    let table_count = tables.len();
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            exporter.generate_and_export(&tables, "er_full.dot", &cache_dir)
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

pub fn write_er_failure_log_blocking(
    failed_tables: Vec<(String, String)>,
    cache_dir: PathBuf,
) -> std::io::Result<()> {
    use std::io::Write;

    let log_path = cache_dir.join("er_diagram.log");
    let mut file = std::fs::File::create(&log_path)?;

    writeln!(file, "ER Diagram Generation Failed")?;
    writeln!(file, "Timestamp: {:?}", std::time::SystemTime::now())?;
    writeln!(file)?;
    writeln!(file, "Failed tables ({}):", failed_tables.len())?;

    for (table, error) in &failed_tables {
        writeln!(file, "  - {}: {}", table, error)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ports::ErExportResult;
    use std::path::Path;
    use std::time::Duration;

    mod spawn_er_diagram_task_tests {
        use super::*;

        struct SuccessExporter;
        impl ErDiagramExporter for SuccessExporter {
            fn generate_and_export(
                &self,
                _tables: &[ErTableInfo],
                _filename: &str,
                _cache_dir: &Path,
            ) -> ErExportResult<PathBuf> {
                Ok(PathBuf::from("/tmp/test.svg"))
            }
        }

        struct FailExporter;
        impl ErDiagramExporter for FailExporter {
            fn generate_and_export(
                &self,
                _tables: &[ErTableInfo],
                _filename: &str,
                _cache_dir: &Path,
            ) -> ErExportResult<PathBuf> {
                Err("export failed".into())
            }
        }

        struct PanicExporter;
        impl ErDiagramExporter for PanicExporter {
            fn generate_and_export(
                &self,
                _tables: &[ErTableInfo],
                _filename: &str,
                _cache_dir: &Path,
            ) -> ErExportResult<PathBuf> {
                panic!("intentional panic")
            }
        }

        #[tokio::test]
        async fn success_sends_opened_action() {
            let (tx, mut rx) = mpsc::channel(1);
            let exporter = Arc::new(SuccessExporter);

            spawn_er_diagram_task(exporter, vec![], 5, PathBuf::from("/tmp"), tx);

            let action = tokio::time::timeout(Duration::from_secs(1), rx.recv())
                .await
                .expect("timeout")
                .expect("channel closed");

            match action {
                Action::ErDiagramOpened {
                    path,
                    table_count,
                    total_tables,
                } => {
                    assert!(path.contains("test.svg"));
                    assert_eq!(table_count, 0);
                    assert_eq!(total_tables, 5);
                }
                _ => panic!("expected ErDiagramOpened, got {:?}", action),
            }
        }

        #[tokio::test]
        async fn error_sends_failed_action() {
            let (tx, mut rx) = mpsc::channel(1);
            let exporter = Arc::new(FailExporter);

            spawn_er_diagram_task(exporter, vec![], 5, PathBuf::from("/tmp"), tx);

            let action = tokio::time::timeout(Duration::from_secs(1), rx.recv())
                .await
                .expect("timeout")
                .expect("channel closed");

            match action {
                Action::ErDiagramFailed(msg) => {
                    assert!(msg.contains("export failed"));
                }
                _ => panic!("expected ErDiagramFailed, got {:?}", action),
            }
        }

        #[tokio::test]
        async fn panic_sends_failed_action() {
            let (tx, mut rx) = mpsc::channel(1);
            let exporter = Arc::new(PanicExporter);

            spawn_er_diagram_task(exporter, vec![], 5, PathBuf::from("/tmp"), tx);

            let action = tokio::time::timeout(Duration::from_secs(1), rx.recv())
                .await
                .expect("timeout")
                .expect("channel closed");

            match action {
                Action::ErDiagramFailed(msg) => {
                    assert!(msg.contains("Task panicked"));
                }
                _ => panic!("expected ErDiagramFailed, got {:?}", action),
            }
        }
    }

    mod write_er_failure_log {
        use super::*;

        #[test]
        fn writes_failure_details_to_log_file() {
            let temp_dir = tempfile::tempdir().unwrap();
            let failed_tables = vec![
                ("public.users".to_string(), "connection timeout".to_string()),
                ("public.orders".to_string(), "permission denied".to_string()),
            ];

            write_er_failure_log_blocking(failed_tables, temp_dir.path().to_path_buf()).unwrap();

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

            write_er_failure_log_blocking(failed_tables, temp_dir.path().to_path_buf()).unwrap();

            let log_path = temp_dir.path().join("er_diagram.log");
            assert!(log_path.exists());

            let content = std::fs::read_to_string(&log_path).unwrap();
            assert!(content.contains("Failed tables (0):"));
        }

        #[test]
        fn includes_timestamp_in_log() {
            let temp_dir = tempfile::tempdir().unwrap();
            let failed_tables = vec![("public.test".to_string(), "error".to_string())];

            write_er_failure_log_blocking(failed_tables, temp_dir.path().to_path_buf()).unwrap();

            let log_path = temp_dir.path().join("er_diagram.log");
            let content = std::fs::read_to_string(&log_path).unwrap();
            assert!(content.contains("Timestamp:"));
        }
    }
}
