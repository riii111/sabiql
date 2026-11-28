use std::sync::Arc;

use crate::domain::connection::ConnectionEndpoint;
use crate::ports::outbound::{DbOperationError, MetadataProvider};
use crate::update::action::Action;

const PREFETCH_TIMEOUT_SECS: u64 = 10;

pub(in crate::cmd) async fn fetch_metadata(
    provider: Arc<dyn MetadataProvider>,
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
) -> Action {
    let outcome = provider.fetch_metadata(&endpoint).await.map(Arc::new);
    Action::MetadataLoaded { run_id, outcome }
}

pub(in crate::cmd) async fn fetch_effective_user(
    provider: Arc<dyn MetadataProvider>,
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
) -> Action {
    let effective_user = provider
        .fetch_effective_user(&endpoint)
        .await
        .ok()
        .flatten();
    Action::EffectiveUserLoaded {
        run_id,
        effective_user,
    }
}

pub(in crate::cmd) async fn fetch_table_detail(
    provider: Arc<dyn MetadataProvider>,
    endpoint: Arc<ConnectionEndpoint>,
    schema: String,
    table: String,
    generation: u64,
    run_id: u64,
) -> Action {
    let outcome = provider
        .fetch_table_detail(&endpoint, &schema, &table)
        .await
        .map(Box::new);
    Action::TableDetailLoaded {
        endpoint,
        run_id,
        outcome,
        generation,
    }
}

pub(in crate::cmd) async fn prefetch_table_detail(
    provider: Arc<dyn MetadataProvider>,
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
    schema: String,
    table: String,
) -> Action {
    let outcome = tokio::time::timeout(
        tokio::time::Duration::from_secs(PREFETCH_TIMEOUT_SECS),
        provider.fetch_table_columns_and_fks(&endpoint, &schema, &table),
    )
    .await
    .unwrap_or_else(|_| Err(DbOperationError::Timeout("timed out".to_string())))
    .map(Box::new);
    Action::TableDetailCached {
        endpoint,
        run_id,
        schema,
        table,
        outcome,
    }
}

pub(in crate::cmd) async fn process_prefetch_queue_after(delay_secs: u64, run_id: u64) -> Action {
    tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
    Action::ProcessPrefetchQueue { run_id }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::test_fixtures;
    use crate::domain::Table;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::test_support::{self, endpoint};

    fn sample_table() -> Table {
        Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            ..test_support::table::minimal("", "")
        }
    }

    #[tokio::test]
    async fn metadata_fetch_reports_provider_outcome() {
        let mut provider = MockMetadataProvider::new();
        provider
            .expect_fetch_metadata()
            .once()
            .returning(|_| Ok(test_fixtures::sample_metadata()));
        provider
            .expect_fetch_metadata()
            .once()
            .returning(|_| Err(DbOperationError::ConnectionFailed("timeout".to_string())));
        let provider = Arc::new(provider) as Arc<dyn MetadataProvider>;

        let loaded = fetch_metadata(Arc::clone(&provider), endpoint::postgres("dsn://a"), 7).await;
        let failed = fetch_metadata(provider, endpoint::postgres("dsn://b"), 8).await;

        assert!(matches!(
            loaded,
            Action::MetadataLoaded {
                run_id: 7,
                outcome: Ok(_)
            }
        ));
        assert!(matches!(
            failed,
            Action::MetadataLoaded {
                run_id: 8,
                outcome: Err(DbOperationError::ConnectionFailed(_))
            }
        ));
    }

    #[tokio::test]
    async fn effective_user_provider_error_is_reported_as_absent() {
        let mut provider = MockMetadataProvider::new();
        provider
            .expect_fetch_effective_user()
            .once()
            .returning(|_| {
                Err(DbOperationError::QueryFailed(
                    "permission denied".to_string(),
                ))
            });

        let action =
            fetch_effective_user(Arc::new(provider), endpoint::postgres("dsn://test"), 7).await;

        assert!(matches!(
            action,
            Action::EffectiveUserLoaded {
                run_id: 7,
                effective_user: None,
            }
        ));
    }

    #[tokio::test]
    async fn table_detail_uses_full_provider() {
        let mut provider = MockMetadataProvider::new();
        provider
            .expect_fetch_table_detail()
            .once()
            .returning(|_, _, _| Ok(sample_table()));
        provider.expect_fetch_table_columns_and_fks().never();

        let action = fetch_table_detail(
            Arc::new(provider),
            endpoint::postgres("dsn://test"),
            "public".to_string(),
            "users".to_string(),
            1,
            9,
        )
        .await;

        assert!(matches!(
            action,
            Action::TableDetailLoaded {
                ref endpoint,
                run_id: 9,
                generation: 1,
                outcome: Ok(_),
            } if endpoint == &endpoint::postgres("dsn://test")
        ));
    }

    #[tokio::test]
    async fn prefetch_uses_light_provider() {
        let mut provider = MockMetadataProvider::new();
        provider.expect_fetch_table_detail().never();
        provider
            .expect_fetch_table_columns_and_fks()
            .once()
            .returning(|_, _, _| Ok(sample_table()));

        let action = prefetch_table_detail(
            Arc::new(provider),
            endpoint::postgres("dsn://test"),
            3,
            "public".to_string(),
            "users".to_string(),
        )
        .await;

        assert!(matches!(
            action,
            Action::TableDetailCached {
                run_id: 3,
                outcome: Ok(_),
                ..
            }
        ));
    }
}
