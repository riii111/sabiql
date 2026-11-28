use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::connection::ConnectionEndpoint;
use crate::ports::outbound::{ConfigWriterError, ErLogWriter, MetadataProvider};
use crate::update::action::{Action, SmartErRefreshError, SmartErRefreshSnapshot};

pub(in crate::cmd) async fn smart_refresh(
    metadata_provider: Arc<dyn MetadataProvider>,
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
) -> Action {
    let outcome = async {
        let new_metadata = metadata_provider
            .fetch_metadata(&endpoint)
            .await
            .map_err(|error| SmartErRefreshError {
                error,
                new_metadata: None,
            })?;
        let new_metadata = Arc::new(new_metadata);
        let signature_snapshot = metadata_provider
            .fetch_table_signatures(&endpoint)
            .await
            .map_err(|error| SmartErRefreshError {
                error,
                new_metadata: Some(Arc::clone(&new_metadata)),
            })?;
        Ok(SmartErRefreshSnapshot {
            new_metadata,
            signature_snapshot: Arc::new(signature_snapshot),
        })
    }
    .await;
    Action::SmartErRefreshFetched {
        endpoint,
        run_id,
        outcome,
    }
}

pub(in crate::cmd) async fn write_failure_log(
    er_log_writer: Arc<dyn ErLogWriter>,
    cache_dir: Result<PathBuf, ConfigWriterError>,
    failed_tables: Vec<(String, String)>,
) -> Option<Action> {
    let result = match cache_dir {
        Ok(cache_dir) => tokio::task::spawn_blocking(move || {
            er_log_writer
                .write_er_failure_log(failed_tables, cache_dir)
                .map_err(|e| e.to_string())
        })
        .await
        .expect("ER failure log task panicked"),
        Err(e) => Err(e.to_string()),
    };
    result.err().map(Action::ErLogWriteFailed)
}
