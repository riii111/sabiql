use std::path::PathBuf;
use std::sync::Arc;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use super::task::spawn_er_diagram_task;
use crate::cmd::effect::Effect;
use crate::domain::ErTableInfo;
use crate::domain::connection::ConnectionEndpoint;
use crate::model::app_state::AppState;
use crate::ports::outbound::{ConfigWriter, ErDiagramExporter, ErLogWriter, MetadataProvider};
use crate::update::action::{Action, SmartErRefreshError, SmartErRefreshFetched};

struct GenerateErDiagramRequest {
    run_id: u64,
    total_tables: usize,
    project_name: String,
    tables: Vec<ErTableInfo>,
    filename: String,
    browser: Option<String>,
}

pub(in crate::cmd) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    er_exporter: &Arc<dyn ErDiagramExporter>,
    config_writer: &Arc<dyn ConfigWriter>,
    er_log_writer: &Arc<dyn ErLogWriter>,
    state: &AppState,
) -> Result<()> {
    match effect {
        Effect::GenerateErDiagram {
            run_id,
            total_tables,
            project_name,
            tables,
            filename,
        } => handle_generate_diagram(
            action_tx,
            er_exporter,
            config_writer,
            GenerateErDiagramRequest {
                run_id,
                total_tables,
                project_name,
                tables,
                filename,
                browser: state.settings.saved_er_browser().map(str::to_string),
            },
        ),
        Effect::WriteErFailureLog { failed_tables } => {
            handle_write_failure_log(
                action_tx,
                config_writer,
                er_log_writer,
                state,
                failed_tables,
            )
            .await
        }

        _ => unreachable!("er::run called with non-er effect"),
    }
}

pub(in crate::cmd) fn smart_refresh_task(
    action_tx: mpsc::Sender<Action>,
    metadata_provider: Arc<dyn MetadataProvider>,
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
) -> impl Future<Output = ()> + Send + 'static {
    let tx = action_tx;

    async move {
        let new_metadata = match metadata_provider.fetch_metadata(&endpoint).await {
            Ok(m) => m,
            Err(e) => {
                tx.send(Action::SmartErRefreshFailed(SmartErRefreshError {
                    endpoint,
                    run_id,
                    error: e,
                    new_metadata: None,
                }))
                .await
                .ok();
                return;
            }
        };

        let signature_snapshot = match metadata_provider.fetch_table_signatures(&endpoint).await {
            Ok(s) => s,
            Err(e) => {
                let new_metadata = Arc::new(new_metadata);
                tx.send(Action::SmartErRefreshFailed(SmartErRefreshError {
                    endpoint,
                    run_id,
                    error: e,
                    new_metadata: Some(Arc::clone(&new_metadata)),
                }))
                .await
                .ok();
                return;
            }
        };

        tx.send(Action::SmartErRefreshFetched(SmartErRefreshFetched {
            endpoint,
            run_id,
            new_metadata: Arc::new(new_metadata),
            signature_snapshot: Arc::new(signature_snapshot),
        }))
        .await
        .ok();
    }
}

fn handle_generate_diagram(
    action_tx: &mpsc::Sender<Action>,
    er_exporter: &Arc<dyn ErDiagramExporter>,
    config_writer: &Arc<dyn ConfigWriter>,
    request: GenerateErDiagramRequest,
) -> Result<()> {
    let GenerateErDiagramRequest {
        run_id,
        total_tables,
        project_name,
        tables,
        filename,
        browser,
    } = request;
    let cache_dir: PathBuf = config_writer.get_cache_dir(&project_name)?;
    spawn_er_diagram_task(
        Arc::clone(er_exporter),
        tables,
        run_id,
        total_tables,
        cache_dir,
        action_tx.clone(),
        filename,
        browser,
    );
    Ok(())
}

async fn handle_write_failure_log(
    action_tx: &mpsc::Sender<Action>,
    config_writer: &Arc<dyn ConfigWriter>,
    er_log_writer: &Arc<dyn ErLogWriter>,
    state: &AppState,
    failed_tables: Vec<(String, String)>,
) -> Result<()> {
    match config_writer.get_cache_dir(state.project_name()) {
        Ok(cache_dir) => {
            let writer = Arc::clone(er_log_writer);
            let tx = action_tx.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(e) = writer.write_er_failure_log(failed_tables, cache_dir) {
                    tx.blocking_send(Action::ErLogWriteFailed(e.to_string()))
                        .ok();
                }
            });
        }
        Err(e) => {
            action_tx
                .send(Action::ErLogWriteFailed(e.to_string()))
                .await
                .ok();
        }
    }
    Ok(())
}
