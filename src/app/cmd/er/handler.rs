use std::sync::Arc;

use crate::domain::connection::ConnectionEndpoint;
use crate::ports::outbound::MetadataProvider;
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
