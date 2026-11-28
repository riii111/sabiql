use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::ErTableInfo;
use crate::ports::outbound::ErDiagramExporter;
use crate::update::action::{Action, ErDiagramInfo};

pub(in crate::cmd) struct ErDiagramRequest {
    pub(in crate::cmd) run_id: u64,
    pub(in crate::cmd) total_tables: usize,
    pub(in crate::cmd) tables: Vec<ErTableInfo>,
    pub(in crate::cmd) cache_dir: PathBuf,
    pub(in crate::cmd) filename: String,
    pub(in crate::cmd) browser: Option<String>,
}

pub(in crate::cmd) async fn generate_er_diagram(
    exporter: Arc<dyn ErDiagramExporter>,
    request: ErDiagramRequest,
) -> Action {
    let ErDiagramRequest {
        run_id,
        total_tables,
        tables,
        cache_dir,
        filename,
        browser,
    } = request;
    let table_count = tables.len();
    let result = tokio::task::spawn_blocking(move || {
        exporter.generate_and_export(&tables, &filename, &cache_dir, browser.as_deref())
    })
    .await;
    let outcome = match result {
        Ok(Ok(path)) => Ok(ErDiagramInfo {
            path: path.display().to_string(),
            table_count,
            total_tables,
        }),
        Ok(Err(e)) => Err(e.to_string()),
        Err(e) => Err(format!("Task panicked: {e}")),
    };
    Action::ErDiagramOpened { run_id, outcome }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::outbound::ErExportResult;
    use std::path::Path;

    mod generate_er_diagram {
        use super::*;

        struct SuccessExporter {
            output_path: PathBuf,
        }

        impl ErDiagramExporter for SuccessExporter {
            fn generate_and_export(
                &self,
                _tables: &[ErTableInfo],
                _filename: &str,
                _cache_dir: &Path,
                _browser: Option<&str>,
            ) -> ErExportResult<PathBuf> {
                Ok(self.output_path.clone())
            }
        }

        struct FailExporter;
        impl ErDiagramExporter for FailExporter {
            fn generate_and_export(
                &self,
                _tables: &[ErTableInfo],
                _filename: &str,
                _cache_dir: &Path,
                _browser: Option<&str>,
            ) -> ErExportResult<PathBuf> {
                Err(std::io::Error::other("export failed").into())
            }
        }

        struct PanicExporter;
        impl ErDiagramExporter for PanicExporter {
            fn generate_and_export(
                &self,
                _tables: &[ErTableInfo],
                _filename: &str,
                _cache_dir: &Path,
                _browser: Option<&str>,
            ) -> ErExportResult<PathBuf> {
                panic!("intentional panic")
            }
        }

        #[tokio::test]
        async fn success_sends_opened_action() {
            let temp_dir = tempfile::tempdir().unwrap();
            let output_path = temp_dir.path().join("test.svg");
            let exporter = Arc::new(SuccessExporter {
                output_path: output_path.clone(),
            });

            let action = super::super::generate_er_diagram(
                exporter,
                ErDiagramRequest {
                    run_id: 1,
                    total_tables: 5,
                    tables: vec![],
                    cache_dir: temp_dir.path().to_path_buf(),
                    filename: "er_full.dot".to_string(),
                    browser: None,
                },
            )
            .await;
            match action {
                Action::ErDiagramOpened {
                    run_id,
                    outcome:
                        Ok(ErDiagramInfo {
                            path,
                            table_count,
                            total_tables,
                        }),
                } => {
                    assert!(path.contains("test.svg"));
                    assert_eq!(table_count, 0);
                    assert_eq!(total_tables, 5);
                    assert_eq!(run_id, 1);
                }
                _ => panic!("expected ErDiagramOpened, got {action:?}"),
            }
        }

        #[tokio::test]
        async fn error_sends_failed_action() {
            let temp_dir = tempfile::tempdir().unwrap();
            let exporter = Arc::new(FailExporter);

            let action = super::super::generate_er_diagram(
                exporter,
                ErDiagramRequest {
                    run_id: 7,
                    total_tables: 5,
                    tables: vec![],
                    cache_dir: temp_dir.path().to_path_buf(),
                    filename: "er_full.dot".to_string(),
                    browser: None,
                },
            )
            .await;
            match action {
                Action::ErDiagramOpened {
                    run_id,
                    outcome: Err(error),
                } => {
                    assert!(error.contains("export failed"));
                    assert_eq!(run_id, 7);
                }
                _ => panic!("expected ErDiagramFailed, got {action:?}"),
            }
        }

        #[tokio::test]
        async fn panic_sends_failed_action() {
            let temp_dir = tempfile::tempdir().unwrap();
            let exporter = Arc::new(PanicExporter);

            let action = super::super::generate_er_diagram(
                exporter,
                ErDiagramRequest {
                    run_id: 11,
                    total_tables: 5,
                    tables: vec![],
                    cache_dir: temp_dir.path().to_path_buf(),
                    filename: "er_full.dot".to_string(),
                    browser: None,
                },
            )
            .await;
            match action {
                Action::ErDiagramOpened {
                    run_id,
                    outcome: Err(error),
                } => {
                    assert!(error.contains("Task panicked"));
                    assert_eq!(run_id, 11);
                }
                _ => panic!("expected ErDiagramFailed, got {action:?}"),
            }
        }
    }
}
